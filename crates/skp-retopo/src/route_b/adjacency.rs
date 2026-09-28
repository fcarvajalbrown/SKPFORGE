use super::dedge::{dedge_next, DirectedGraph, INVALID};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Link {
    pub id: u32,
    pub weight: f64,
}

impl Link {
    pub fn new(id: u32) -> Link {
        Link { id, weight: 1.0 }
    }
}

pub type Adjacency = Vec<Vec<Link>>;

pub fn uniform_adjacency(faces: &[[u32; 3]], graph: &DirectedGraph) -> Adjacency {
    let mut adj: Adjacency = vec![Vec::new(); graph.v2e.len()];
    for (i, links) in adj.iter_mut().enumerate() {
        let start = graph.v2e[i];
        if start == INVALID {
            continue;
        }
        let mut edge = start;
        loop {
            let base = (edge % 3) as usize;
            let face = &faces[(edge / 3) as usize];
            let opp = graph.e2e[edge as usize];
            if links.is_empty() {
                links.push(Link::new(face[(base + 2) % 3]));
            }
            if opp == INVALID {
                links.push(Link::new(face[(base + 1) % 3]));
                break;
            }
            let next = dedge_next(opp, 3);
            if next != start {
                links.push(Link::new(face[(base + 1) % 3]));
            }
            edge = next;
            if edge == start {
                break;
            }
        }
    }
    adj
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(links: &[Link]) -> Vec<u32> {
        links.iter().map(|l| l.id).collect()
    }

    #[test]
    fn a_boundary_fan_lists_every_neighbour_once_in_winding_order() {
        let faces = [[0, 1, 2], [0, 2, 3]];
        let adj = uniform_adjacency(&faces, &DirectedGraph::build(4, &faces));
        assert_eq!(ids(&adj[0]), [3, 2, 1]);
        assert_eq!(ids(&adj[2]), [1, 0, 3]);
        assert_eq!(ids(&adj[1]), [0, 2]);
        assert!(adj.iter().flatten().all(|l| l.weight == 1.0));
    }

    #[test]
    fn a_closed_fan_lists_every_neighbour_once() {
        let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let adj = uniform_adjacency(&faces, &DirectedGraph::build(4, &faces));
        for (v, links) in adj.iter().enumerate() {
            let mut got = ids(links);
            got.sort();
            let want: Vec<u32> = (0..4).filter(|&u| u != v as u32).collect();
            assert_eq!(got, want);
        }
    }

    #[test]
    fn a_non_manifold_vertex_has_no_neighbours() {
        let faces = [[0, 1, 2], [1, 0, 3], [1, 0, 4]];
        let adj = uniform_adjacency(&faces, &DirectedGraph::build(5, &faces));
        assert!(adj[0].is_empty());
        assert!(adj[1].is_empty());
        assert!(!adj[2].is_empty());
    }
}
