use super::dedge::{dedge_next, dedge_prev, INVALID};
use skp_core::geometry::Vec3;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Debug, Clone, Copy)]
struct Entry {
    length: f64,
    edge: u32,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Entry) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Entry) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Entry) -> Ordering {
        self.length
            .total_cmp(&other.length)
            .then(self.edge.cmp(&other.edge))
    }
}

fn squared_length(positions: &[Vec3], a: u32, b: u32) -> f64 {
    let d = positions[a as usize] - positions[b as usize];
    d.dot(d)
}

fn too_long(length: f64, max_squared: f64, rho_a: f64, rho_b: f64) -> bool {
    length > max_squared || length > (max_squared * 0.75).max(rho_a.min(rho_b))
}

fn link(e2e: &mut [u32], a: u32, b: u32) {
    e2e[a as usize] = b;
    if b != INVALID {
        e2e[b as usize] = a;
    }
}

pub struct Subdivided {
    pub faces: Vec<[u32; 3]>,
    pub positions: Vec<Vec3>,
    pub rho: Vec<f64>,
    pub e2e: Vec<u32>,
    pub splits: usize,
}

pub fn subdivide(
    faces: Vec<[u32; 3]>,
    positions: Vec<Vec3>,
    rho: Vec<f64>,
    e2e: Vec<u32>,
    non_manifold: &[bool],
    max_length: f64,
) -> Subdivided {
    let mut s = Subdivided {
        faces,
        positions,
        rho,
        e2e,
        splits: 0,
    };
    let max_squared = max_length * max_length;
    let mut queue = BinaryHeap::new();

    for i in 0..s.e2e.len() as u32 {
        let face = s.faces[(i / 3) as usize];
        let (v0, v1) = (face[(i % 3) as usize], face[((i + 1) % 3) as usize]);
        if non_manifold[v0 as usize] || non_manifold[v1 as usize] {
            continue;
        }
        let length = squared_length(&s.positions, v0, v1);
        if too_long(length, max_squared, s.rho[v0 as usize], s.rho[v1 as usize]) {
            let other = s.e2e[i as usize];
            if other == INVALID || other > i {
                queue.push(Entry { length, edge: i });
            }
        }
    }

    while let Some(Entry { length, edge: e0 }) = queue.pop() {
        let e1 = s.e2e[e0 as usize];
        let is_boundary = e1 == INVALID;
        let f0 = (e0 / 3) as usize;
        let at = |e: u32, k: u32| ((e + k) % 3) as usize;
        let (v0, v0p, v1) = (
            s.faces[f0][at(e0, 0)],
            s.faces[f0][at(e0, 2)],
            s.faces[f0][at(e0, 1)],
        );
        if squared_length(&s.positions, v0, v1) != length {
            continue;
        }
        let f1 = if is_boundary {
            usize::MAX
        } else {
            (e1 / 3) as usize
        };
        let v1p = if is_boundary {
            INVALID
        } else {
            s.faces[f1][at(e1, 2)]
        };

        let vn = s.positions.len() as u32;
        s.splits += 1;
        s.positions
            .push((s.positions[v0 as usize] + s.positions[v1 as usize]) * 0.5);
        s.rho.push(0.5 * s.rho[v1 as usize]);

        let f2 = if is_boundary {
            usize::MAX
        } else {
            s.faces.push([0; 3]);
            s.faces.len() - 1
        };
        s.faces.push([0; 3]);
        let f3 = s.faces.len() - 1;
        s.e2e.resize(s.faces.len() * 3, INVALID);

        s.faces[f0] = [vn, v0p, v0];
        if !is_boundary {
            s.faces[f1] = [vn, v0, v1p];
            s.faces[f2] = [vn, v1p, v1];
        }
        s.faces[f3] = [vn, v1, v0p];

        let e0p = s.e2e[dedge_prev(e0, 3) as usize];
        let e0n = s.e2e[dedge_next(e0, 3) as usize];
        let (f0, f3) = (f0 as u32, f3 as u32);
        let e2e = &mut s.e2e;
        link(e2e, 3 * f0, 3 * f3 + 2);
        link(e2e, 3 * f0 + 1, e0p);
        link(e2e, 3 * f3 + 1, e0n);
        if is_boundary {
            link(e2e, 3 * f0 + 2, INVALID);
            link(e2e, 3 * f3, INVALID);
        } else {
            let (f1, f2) = (f1 as u32, f2 as u32);
            let e1p = e2e[dedge_prev(e1, 3) as usize];
            let e1n = e2e[dedge_next(e1, 3) as usize];
            link(e2e, 3 * f0 + 2, 3 * f1);
            link(e2e, 3 * f1 + 1, e1n);
            link(e2e, 3 * f1 + 2, 3 * f2);
            link(e2e, 3 * f2 + 1, e1p);
            link(e2e, 3 * f2 + 2, 3 * f3);
        }

        let mut schedule = |f: u32| {
            for i in 0..3 {
                let face = s.faces[f as usize];
                let (a, b) = (face[i], face[(i + 1) % 3]);
                let length = squared_length(&s.positions, a, b);
                if too_long(length, max_squared, s.rho[a as usize], s.rho[b as usize]) {
                    queue.push(Entry {
                        length,
                        edge: f * 3 + i as u32,
                    });
                }
            }
        };
        schedule(f0);
        if !is_boundary {
            schedule(f2 as u32);
            schedule(f1 as u32);
        }
        schedule(f3);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_b::dedge::DirectedGraph;
    use skp_core::geometry::area_vector;

    fn p(x: f64, y: f64) -> Vec3 {
        Vec3::new(x, y, 0.0)
    }

    fn run(faces: Vec<[u32; 3]>, positions: Vec<Vec3>, rho: Vec<f64>, max: f64) -> Subdivided {
        let g = DirectedGraph::build(positions.len(), &faces);
        subdivide(faces, positions, rho, g.e2e, &g.non_manifold, max)
    }

    fn area(s: &Subdivided) -> f64 {
        s.faces
            .iter()
            .map(|f| {
                let [a, b, c] = f.map(|v| s.positions[v as usize]);
                area_vector(a, b, c).length() * 0.5
            })
            .sum()
    }

    fn assert_twins_consistent(s: &Subdivided) {
        for (e, &t) in s.e2e.iter().enumerate() {
            if t == INVALID {
                continue;
            }
            assert_eq!(s.e2e[t as usize], e as u32);
            let edge = |e: usize| {
                let f = s.faces[e / 3];
                (f[e % 3], f[(e + 1) % 3])
            };
            let (a, b) = edge(e);
            assert_eq!(edge(t as usize), (b, a));
        }
    }

    #[test]
    fn a_long_boundary_edge_is_split_at_its_midpoint_with_upstream_face_order() {
        let s = run(
            vec![[0, 1, 2]],
            vec![p(0.0, 0.0), p(1.0, 0.0), p(0.0, 1.0)],
            vec![1.0, 1.0, 3.0],
            1.2,
        );
        assert_eq!(s.splits, 1);
        assert_eq!(s.faces, [[3, 0, 1], [3, 2, 0]]);
        assert_eq!(s.positions[3], p(0.5, 0.5));
        assert_eq!(s.e2e, [5, INVALID, INVALID, INVALID, INVALID, 0]);
        assert_eq!(s.rho[3], 1.5);
    }

    #[test]
    fn a_long_interior_edge_splits_both_faces() {
        let s = run(
            vec![[0, 1, 2], [0, 2, 3]],
            vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            vec![1.0; 4],
            1.2,
        );
        assert_eq!(s.splits, 1);
        assert_eq!(s.faces.len(), 4);
        assert_eq!(s.positions[4], p(0.5, 0.5));
        assert_twins_consistent(&s);
        assert!((area(&s) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn every_edge_ends_within_the_bound_and_the_surface_is_unchanged() {
        let s = run(
            vec![[0, 1, 2], [0, 2, 3]],
            vec![p(0.0, 0.0), p(8.0, 0.0), p(8.0, 3.0), p(0.0, 3.0)],
            vec![1.0; 4],
            1.0,
        );
        assert!(s.splits > 10);
        assert_twins_consistent(&s);
        assert!((area(&s) - 24.0).abs() < 1e-9);
        for (f, face) in s.faces.iter().enumerate() {
            for i in 0..3 {
                let (a, b) = (face[i], face[(i + 1) % 3]);
                let length = squared_length(&s.positions, a, b);
                assert!(
                    !too_long(length, 1.0, s.rho[a as usize], s.rho[b as usize]),
                    "face {f} edge {i} squared length {length}"
                );
            }
        }
    }

    #[test]
    fn edges_touching_a_non_manifold_vertex_are_not_queued() {
        let s = run(
            vec![[0, 1, 2], [1, 0, 3], [1, 0, 4]],
            vec![
                p(0.0, 0.0),
                p(10.0, 0.0),
                p(5.0, 1.0),
                p(5.0, -1.0),
                Vec3::new(5.0, 0.0, 1.0),
            ],
            vec![100.0; 5],
            1.0,
        );
        assert_eq!(s.splits, 0);
    }
}
