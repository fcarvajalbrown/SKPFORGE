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
pub struct EcMaxFlow {
    graph: Vec<Vec<Arc>>,
}

struct Visit {
    node: u32,
    prev: usize,
    arc: (u32, u32),
}

impl EcMaxFlow {
    pub fn new(nodes: usize) -> EcMaxFlow {
        EcMaxFlow {
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

    pub fn compute(&mut self) -> i32 {
        let sink = self.graph.len() as u32 - 1;
        let mut total = 0;
        loop {
            let mut seen = vec![false; self.graph.len()];
            let mut queue = vec![Visit {
                node: 0,
                prev: usize::MAX,
                arc: (0, 0),
            }];
            seen[0] = true;
            let mut front = 0;
            let mut found = false;
            while front < queue.len() {
                let node = queue[front].node;
                for (k, arc) in self.graph[node as usize].iter().enumerate() {
                    if seen[arc.to as usize] || arc.capacity <= arc.flow {
                        continue;
                    }
                    queue.push(Visit {
                        node: arc.to,
                        prev: front,
                        arc: (node, k as u32),
                    });
                    seen[arc.to as usize] = true;
                    if arc.to == sink {
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
                front += 1;
            }
            if !found {
                return total;
            }
            let mut at = queue.len() - 1;
            while queue[at].prev != usize::MAX {
                let (node, k) = queue[at].arc;
                let arc = &mut self.graph[node as usize][k as usize];
                arc.flow += 1;
                let (rn, rk) = arc.rev;
                self.graph[rn as usize][rk as usize].flow -= 1;
                at = queue[at].prev;
            }
            total += 1;
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
    fn two_disjoint_paths_carry_two_units() {
        let mut f = EcMaxFlow::new(4);
        f.add_edge(0, 1, 1, 0, -1);
        f.add_edge(0, 2, 1, 0, -1);
        f.add_edge(1, 3, 1, 0, -1);
        f.add_edge(2, 3, 1, 0, -1);
        assert_eq!(f.compute(), 2);
    }

    #[test]
    fn a_bottleneck_limits_the_flow() {
        let mut f = EcMaxFlow::new(4);
        f.add_edge(0, 1, 5, 0, -1);
        f.add_edge(1, 2, 2, 0, -1);
        f.add_edge(2, 3, 5, 0, -1);
        assert_eq!(f.compute(), 2);
    }

    #[test]
    fn flow_can_be_rerouted_through_a_reverse_arc() {
        let mut f = EcMaxFlow::new(4);
        f.add_edge(0, 1, 1, 0, -1);
        f.add_edge(0, 2, 1, 0, -1);
        f.add_edge(1, 2, 1, 0, -1);
        f.add_edge(1, 3, 1, 0, -1);
        f.add_edge(2, 3, 1, 0, -1);
        assert_eq!(f.compute(), 2);
    }

    #[test]
    fn positive_flow_moves_the_edge_difference_by_the_arc_sign() {
        let mut f = EcMaxFlow::new(4);
        f.add_edge(0, 1, 1, 0, -1);
        f.add_edge(1, 2, 3, 3, 4);
        f.add_edge(2, 1, 3, 3, 1);
        f.add_edge(2, 3, 1, 0, -1);
        assert_eq!(f.compute(), 1);
        let mut diff = vec![[0, 0]; 3];
        f.apply_to(&mut diff);
        assert_eq!(diff, [[0, 0], [0, 0], [-1, 0]]);
    }
}
