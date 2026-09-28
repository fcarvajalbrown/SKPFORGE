use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Incidence {
    pub face: u32,
    pub forward: bool,
}

pub struct Edges {
    map: HashMap<(u32, u32), Vec<Incidence>>,
}

pub fn triangle_edges([a, b, c]: [u32; 3]) -> [(u32, u32); 3] {
    [(a, b), (b, c), (c, a)]
}

fn key(from: u32, to: u32) -> ((u32, u32), bool) {
    if from < to {
        ((from, to), true)
    } else {
        ((to, from), false)
    }
}

impl Edges {
    pub fn build(triangles: &[[u32; 3]]) -> Edges {
        let mut map: HashMap<(u32, u32), Vec<Incidence>> = HashMap::new();
        for (face, &tri) in triangles.iter().enumerate() {
            for (from, to) in triangle_edges(tri) {
                let (k, forward) = key(from, to);
                map.entry(k).or_default().push(Incidence {
                    face: face as u32,
                    forward,
                });
            }
        }
        Edges { map }
    }

    pub fn around(&self, from: u32, to: u32) -> &[Incidence] {
        self.map
            .get(&key(from, to).0)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn degree(&self, from: u32, to: u32) -> usize {
        self.around(from, to).len()
    }

    pub fn other(&self, from: u32, to: u32, face: u32) -> Option<Incidence> {
        match self.around(from, to) {
            [a, b] if a.face == face => Some(*b),
            [a, b] if b.face == face => Some(*a),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_edge_is_found_from_either_end() {
        let edges = Edges::build(&[[0, 1, 2], [2, 1, 3]]);
        assert_eq!(edges.degree(1, 2), 2);
        assert_eq!(edges.degree(2, 1), 2);
        assert_eq!(edges.degree(0, 3), 0);
    }

    #[test]
    fn consistent_neighbours_cross_a_shared_edge_in_opposite_directions() {
        let edges = Edges::build(&[[0, 1, 2], [2, 1, 3]]);
        let around = edges.around(1, 2);
        assert_ne!(around[0].forward, around[1].forward);
    }

    #[test]
    fn the_other_side_exists_only_on_a_manifold_edge() {
        let edges = Edges::build(&[[0, 1, 2], [2, 1, 3], [1, 2, 4]]);
        assert_eq!(edges.other(0, 1, 0), None);
        assert_eq!(edges.other(1, 2, 0), None);
        let manifold = Edges::build(&[[0, 1, 2], [2, 1, 3]]);
        assert_eq!(manifold.other(1, 2, 0).map(|i| i.face), Some(1));
    }
}
