use skp_core::progress::{CancelToken, Cancelled};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Arc {
    to: u32,
    capacity: i32,
    flow: i32,
    variable: i32,
    sign: i32,
    rev: (u32, u32),
}

const FREE: u8 = 0;
const SOURCE_TREE: u8 = 1;
const SINK_TREE: u8 = 2;
const NO_PARENT: (u32, u32) = (u32::MAX, u32::MAX);

struct Meet {
    source_side: u32,
    sink_side: u32,
    arc: (u32, u32),
}

struct Trees {
    tag: Vec<u8>,
    parent: Vec<(u32, u32)>,
    stamp: Vec<u64>,
    dist: Vec<u32>,
    active: VecDeque<u32>,
    in_active: Vec<bool>,
    orphans: VecDeque<u32>,
    time: u64,
    sink: u32,
}

impl Trees {
    fn new(n: usize) -> Trees {
        let sink = n as u32 - 1;
        let mut t = Trees {
            tag: vec![FREE; n],
            parent: vec![NO_PARENT; n],
            stamp: vec![0; n],
            dist: vec![0; n],
            active: VecDeque::new(),
            in_active: vec![false; n],
            orphans: VecDeque::new(),
            time: 1,
            sink,
        };
        t.tag[0] = SOURCE_TREE;
        t.tag[sink as usize] = SINK_TREE;
        t.activate(0);
        t.activate(sink);
        t
    }

    fn activate(&mut self, v: u32) {
        if !self.in_active[v as usize] {
            self.in_active[v as usize] = true;
            self.active.push_back(v);
        }
    }

    fn is_terminal(&self, v: u32) -> bool {
        v == 0 || v == self.sink
    }

    fn parent_node(&self, flow: &MaxFlow, v: u32) -> u32 {
        let (owner, k) = self.parent[v as usize];
        if self.tag[v as usize] == SOURCE_TREE {
            owner
        } else {
            flow.graph[owner as usize][k as usize].to
        }
    }

    fn path(&self, flow: &MaxFlow, meet: Meet) -> Vec<(u32, u32)> {
        let mut path = Vec::new();
        let mut v = meet.source_side;
        while !self.is_terminal(v) {
            path.push(self.parent[v as usize]);
            v = self.parent_node(flow, v);
        }
        path.reverse();
        path.push(meet.arc);
        let mut v = meet.sink_side;
        while !self.is_terminal(v) {
            path.push(self.parent[v as usize]);
            v = self.parent_node(flow, v);
        }
        path
    }

    fn orphan_below(&mut self, flow: &MaxFlow, arc: (u32, u32)) {
        let head = flow.graph[arc.0 as usize][arc.1 as usize].to;
        let child = if self.tag[head as usize] == SOURCE_TREE && self.parent[head as usize] == arc {
            head
        } else if self.tag[arc.0 as usize] == SINK_TREE && self.parent[arc.0 as usize] == arc {
            arc.0
        } else {
            return;
        };
        self.parent[child as usize] = NO_PARENT;
        self.orphans.push_back(child);
    }

    fn origin(&mut self, flow: &MaxFlow, q: u32) -> Option<u32> {
        let mut d = 0;
        let mut v = q;
        loop {
            if self.stamp[v as usize] == self.time {
                d += self.dist[v as usize];
                break;
            }
            if self.is_terminal(v) {
                break;
            }
            if self.parent[v as usize] == NO_PARENT {
                return None;
            }
            v = self.parent_node(flow, v);
            d += 1;
        }
        let mut v = q;
        let mut dv = d;
        while self.stamp[v as usize] != self.time {
            self.stamp[v as usize] = self.time;
            self.dist[v as usize] = dv;
            if self.is_terminal(v) {
                break;
            }
            v = self.parent_node(flow, v);
            dv -= 1;
        }
        Some(d)
    }
}

#[derive(Debug, Clone, Default)]
pub struct MaxFlow {
    graph: Vec<Vec<Arc>>,
}

impl MaxFlow {
    pub fn new(nodes: usize) -> MaxFlow {
        MaxFlow {
            graph: vec![Vec::new(); nodes],
        }
    }

    pub fn add_edge(
        &mut self,
        x: u32,
        y: u32,
        capacity: i32,
        reverse_capacity: i32,
        variable: i32,
    ) {
        let xi = self.graph[x as usize].len() as u32;
        let yi = self.graph[y as usize].len() as u32 + u32::from(x == y);
        self.graph[x as usize].push(Arc {
            to: y,
            capacity,
            flow: 0,
            variable,
            sign: -1,
            rev: (y, yi),
        });
        self.graph[y as usize].push(Arc {
            to: x,
            capacity: reverse_capacity,
            flow: 0,
            variable,
            sign: 1,
            rev: (x, xi),
        });
    }

    fn residual(&self, (u, k): (u32, u32)) -> i32 {
        let arc = &self.graph[u as usize][k as usize];
        arc.capacity - arc.flow
    }

    fn push_along(&mut self, path: &[(u32, u32)]) -> i32 {
        let bottleneck = path.iter().map(|&a| self.residual(a)).min().unwrap_or(0);
        for &(u, k) in path {
            let arc = &mut self.graph[u as usize][k as usize];
            arc.flow += bottleneck;
            let (rn, rk) = arc.rev;
            self.graph[rn as usize][rk as usize].flow -= bottleneck;
        }
        bottleneck
    }

    pub fn compute(&mut self, cancel: &CancelToken) -> Result<i32, Cancelled> {
        let mut trees = Trees::new(self.graph.len());
        let mut total = 0;
        let mut augmentations = 0u32;
        loop {
            cancel.check()?;
            let Some(meet) = self.grow(&mut trees) else {
                return Ok(total);
            };
            let path = trees.path(self, meet);
            total += self.push_along(&path);
            augmentations += 1;
            if augmentations.is_multiple_of(1024) {
                cancel.check()?;
            }
            trees.time += 1;
            for &arc in &path {
                if self.residual(arc) == 0 {
                    trees.orphan_below(self, arc);
                }
            }
            self.adopt(&mut trees);
        }
    }

    fn tree_arc(&self, tag: u8, p: u32, k: u32) -> (u32, u32) {
        if tag == SOURCE_TREE {
            (p, k)
        } else {
            self.graph[p as usize][k as usize].rev
        }
    }

    fn grow(&self, t: &mut Trees) -> Option<Meet> {
        while let Some(&p) = t.active.front() {
            let tag = t.tag[p as usize];
            if tag == FREE {
                t.active.pop_front();
                t.in_active[p as usize] = false;
                continue;
            }
            for k in 0..self.graph[p as usize].len() as u32 {
                let arc = self.tree_arc(tag, p, k);
                if self.residual(arc) <= 0 {
                    continue;
                }
                let q = self.graph[p as usize][k as usize].to;
                let other = t.tag[q as usize];
                if other == FREE {
                    t.tag[q as usize] = tag;
                    t.parent[q as usize] = arc;
                    t.stamp[q as usize] = t.stamp[p as usize];
                    t.dist[q as usize] = t.dist[p as usize] + 1;
                    t.activate(q);
                } else if other != tag {
                    return Some(if tag == SOURCE_TREE {
                        Meet {
                            source_side: p,
                            sink_side: q,
                            arc,
                        }
                    } else {
                        Meet {
                            source_side: q,
                            sink_side: p,
                            arc,
                        }
                    });
                }
            }
            t.active.pop_front();
            t.in_active[p as usize] = false;
        }
        None
    }

    fn adopt(&self, t: &mut Trees) {
        while let Some(p) = t.orphans.pop_front() {
            let tag = t.tag[p as usize];
            let mut best: Option<((u32, u32), u32)> = None;
            for k in 0..self.graph[p as usize].len() as u32 {
                let q = self.graph[p as usize][k as usize].to;
                if t.tag[q as usize] != tag {
                    continue;
                }
                let arc = if tag == SOURCE_TREE {
                    self.graph[p as usize][k as usize].rev
                } else {
                    (p, k)
                };
                if self.residual(arc) <= 0 {
                    continue;
                }
                if let Some(d) = t.origin(self, q) {
                    if best.is_none_or(|(_, bd)| d < bd) {
                        best = Some((arc, d));
                    }
                }
            }
            if let Some((arc, d)) = best {
                t.parent[p as usize] = arc;
                t.stamp[p as usize] = t.time;
                t.dist[p as usize] = d + 1;
                continue;
            }
            for k in 0..self.graph[p as usize].len() as u32 {
                let q = self.graph[p as usize][k as usize].to;
                if t.tag[q as usize] != tag {
                    continue;
                }
                let toward_p = if tag == SOURCE_TREE {
                    self.graph[p as usize][k as usize].rev
                } else {
                    (p, k)
                };
                if self.residual(toward_p) > 0 {
                    t.activate(q);
                }
                if t.parent[q as usize] != NO_PARENT && t.parent_node(self, q) == p {
                    t.parent[q as usize] = NO_PARENT;
                    t.orphans.push_back(q);
                }
            }
            t.tag[p as usize] = FREE;
        }
    }

    pub fn apply_to(&self, edge_diff: &mut [[i32; 2]]) {
        for arcs in &self.graph {
            for arc in arcs {
                if arc.flow > 0 && arc.variable != -1 {
                    let v = arc.variable as usize;
                    edge_diff[v / 2][v % 2] += arc.sign * arc.flow;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_token_stops_the_flow() {
        let mut f = MaxFlow::new(2);
        f.add_edge(0, 1, 1, 0, -1);
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(f.compute(&cancel), Err(Cancelled));
    }

    #[test]
    fn two_disjoint_paths_carry_two_units() {
        let mut f = MaxFlow::new(4);
        f.add_edge(0, 1, 1, 0, -1);
        f.add_edge(0, 2, 1, 0, -1);
        f.add_edge(1, 3, 1, 0, -1);
        f.add_edge(2, 3, 1, 0, -1);
        assert_eq!(f.compute(&CancelToken::new()).unwrap(), 2);
    }

    #[test]
    fn a_bottleneck_limits_the_flow() {
        let mut f = MaxFlow::new(4);
        f.add_edge(0, 1, 5, 0, -1);
        f.add_edge(1, 2, 2, 0, -1);
        f.add_edge(2, 3, 5, 0, -1);
        assert_eq!(f.compute(&CancelToken::new()).unwrap(), 2);
    }

    #[test]
    fn flow_can_be_rerouted_through_a_reverse_arc() {
        let mut f = MaxFlow::new(4);
        f.add_edge(0, 1, 1, 0, -1);
        f.add_edge(0, 2, 1, 0, -1);
        f.add_edge(1, 2, 1, 0, -1);
        f.add_edge(1, 3, 1, 0, -1);
        f.add_edge(2, 3, 1, 0, -1);
        assert_eq!(f.compute(&CancelToken::new()).unwrap(), 2);
    }

    #[test]
    fn positive_flow_moves_the_edge_difference_by_the_arc_sign() {
        let mut f = MaxFlow::new(4);
        f.add_edge(0, 1, 1, 0, -1);
        f.add_edge(1, 2, 3, 3, 4);
        f.add_edge(2, 1, 3, 3, 1);
        f.add_edge(2, 3, 1, 0, -1);
        assert_eq!(f.compute(&CancelToken::new()).unwrap(), 1);
        let mut diff = vec![[0, 0]; 3];
        f.apply_to(&mut diff);
        assert_eq!(diff, [[0, 0], [0, 0], [-1, 0]]);
    }

    fn reachable(f: &MaxFlow) -> Vec<u32> {
        let mut seen = vec![u32::MAX; f.graph.len()];
        let mut queue = VecDeque::from([0u32]);
        seen[0] = 0;
        while let Some(u) = queue.pop_front() {
            for arc in &f.graph[u as usize] {
                if arc.capacity > arc.flow && seen[arc.to as usize] == u32::MAX {
                    seen[arc.to as usize] = 0;
                    queue.push_back(arc.to);
                }
            }
        }
        seen
    }

    #[test]
    fn the_flow_equals_the_cut_it_leaves_on_random_networks() {
        use crate::route_b::pcg32::Pcg32;
        let mut rng = Pcg32::seeded(11, 3);
        for _ in 0..50 {
            let n = 2 + rng.next_u32_below(30) as usize;
            let mut f = MaxFlow::new(n);
            for _ in 0..rng.next_u32_below(120) {
                let (x, y) = (rng.next_u32_below(n as u32), rng.next_u32_below(n as u32));
                if x != y {
                    let c = rng.next_u32_below(5) as i32;
                    let rc = rng.next_u32_below(3) as i32;
                    f.add_edge(x, y, c, rc, -1);
                }
            }
            let flow = f.compute(&CancelToken::new()).unwrap();
            let reach = reachable(&f);
            let mut cut = 0;
            let mut net = vec![0i32; n];
            for (u, arcs) in f.graph.iter().enumerate() {
                for arc in arcs {
                    assert!(arc.flow <= arc.capacity);
                    net[u] += arc.flow;
                    if reach[u] != u32::MAX && reach[arc.to as usize] == u32::MAX {
                        cut += arc.capacity;
                    }
                }
            }
            assert_eq!(flow, cut);
            assert_eq!(net[0], flow);
            assert_eq!(net[n - 1], -flow);
            assert!(net[1..n - 1].iter().all(|&x| x == 0));
        }
    }
}
