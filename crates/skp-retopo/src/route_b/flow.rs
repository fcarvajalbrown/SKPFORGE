use skp_core::progress::{CancelToken, Cancelled};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Arc {
    to: u32,
    capacity: i32,
    flow: i32,
    variable: i32,
    sign: i32,
    rev: (u32, u32),
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

    fn levels(&self) -> Vec<u32> {
        let mut level = vec![u32::MAX; self.graph.len()];
        let mut queue = std::collections::VecDeque::from([0u32]);
        level[0] = 0;
        let sink = self.graph.len() - 1;
        while let Some(u) = queue.pop_front() {
            if level[sink] != u32::MAX && level[u as usize] >= level[sink] {
                break;
            }
            for arc in &self.graph[u as usize] {
                if arc.capacity > arc.flow && level[arc.to as usize] == u32::MAX {
                    level[arc.to as usize] = level[u as usize] + 1;
                    queue.push_back(arc.to);
                }
            }
        }
        level
    }

    fn push_along(&mut self, path: &[(u32, u32)]) -> i32 {
        let bottleneck = path
            .iter()
            .map(|&(u, k)| {
                let arc = &self.graph[u as usize][k as usize];
                arc.capacity - arc.flow
            })
            .min()
            .unwrap_or(0);
        for &(u, k) in path {
            let arc = &mut self.graph[u as usize][k as usize];
            arc.flow += bottleneck;
            let (rn, rk) = arc.rev;
            self.graph[rn as usize][rk as usize].flow -= bottleneck;
        }
        bottleneck
    }

    pub fn compute(&mut self, cancel: &CancelToken) -> Result<i32, Cancelled> {
        let sink = self.graph.len() as u32 - 1;
        let mut total = 0;
        loop {
            cancel.check()?;
            let mut level = self.levels();
            if level[sink as usize] == u32::MAX {
                return Ok(total);
            }
            let mut next_arc = vec![0usize; self.graph.len()];
            let mut path: Vec<(u32, u32)> = Vec::new();
            let mut augmentations = 0u32;
            loop {
                let u = path
                    .last()
                    .map_or(0, |&(n, k)| self.graph[n as usize][k as usize].to);
                if u == sink {
                    total += self.push_along(&path);
                    path.clear();
                    augmentations += 1;
                    if augmentations.is_multiple_of(1024) {
                        cancel.check()?;
                    }
                    continue;
                }
                let arcs = &self.graph[u as usize];
                let mut advanced = false;
                while next_arc[u as usize] < arcs.len() {
                    let k = next_arc[u as usize];
                    let arc = &arcs[k];
                    if arc.capacity > arc.flow && level[arc.to as usize] == level[u as usize] + 1 {
                        path.push((u, k as u32));
                        advanced = true;
                        break;
                    }
                    next_arc[u as usize] += 1;
                }
                if advanced {
                    continue;
                }
                level[u as usize] = u32::MAX;
                match path.pop() {
                    Some((n, _)) => next_arc[n as usize] += 1,
                    None => break,
                }
            }
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
            let reach = f.levels();
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
