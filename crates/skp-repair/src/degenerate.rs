use crate::compact::retain_faces;
use crate::geometry::{height, triangle_points, triangle_positions, Vec3};
use crate::topology::Edges;
use skp_core::mesh::{Corner, Face, Mesh, Normal, Uvq};
use skp_core::units::Uu;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Degenerates {
    pub dropped: usize,
    pub collapsed: usize,
    pub neighbours_split: usize,
}

pub fn is_degenerate(mesh: &Mesh, face: usize, tolerance: Uu) -> bool {
    let [a, b, c] = triangle_positions(mesh, face);
    if a == b || b == c || c == a {
        return true;
    }
    let [pa, pb, pc] = triangle_points(mesh, face);
    height(pa, pb, pc) < tolerance.0
}

fn long_edge_and_middle(mesh: &Mesh, face: usize) -> Option<(u32, u32, u32)> {
    let tri = triangle_positions(mesh, face);
    if tri[0] == tri[1] || tri[1] == tri[2] || tri[2] == tri[0] {
        return None;
    }
    let points = triangle_points(mesh, face);
    let longest = (0..3)
        .max_by(|&i, &j| {
            let len = |k: usize| (points[(k + 1) % 3] - points[k]).length();
            len(i).total_cmp(&len(j))
        })
        .unwrap_or(0);
    Some((tri[longest], tri[(longest + 1) % 3], tri[(longest + 2) % 3]))
}

fn lerp_uvq(a: Uvq, b: Uvq, s: f64) -> Uvq {
    Uvq {
        u: a.u + (b.u - a.u) * s,
        v: a.v + (b.v - a.v) * s,
        q: a.q + (b.q - a.q) * s,
    }
}

fn lerp_normal(a: Normal, b: Normal, s: f64) -> Normal {
    let n = Vec3::new(a.x, a.y, a.z) * (1.0 - s) + Vec3::new(b.x, b.y, b.z) * s;
    match n.normalised() {
        Some(n) => Normal {
            x: n.x,
            y: n.y,
            z: n.z,
        },
        None => a,
    }
}

fn split(mesh: &mut Mesh, face: usize, a: u32, b: u32, middle: u32) -> bool {
    let c = mesh.faces[face].corners();
    let corners = [c[0], c[1], c[2]];
    let positions = corners.map(|k| mesh.corners[k as usize].position);
    let Some(k) = (0..3).find(|&k| {
        let (from, to) = (positions[k], positions[(k + 1) % 3]);
        (from == a && to == b) || (from == b && to == a)
    }) else {
        return false;
    };
    let (x, y, d) = (corners[k], corners[(k + 1) % 3], corners[(k + 2) % 3]);
    let (cx, cy) = (mesh.corners[x as usize], mesh.corners[y as usize]);
    let px = Vec3::of(mesh.positions[cx.position as usize]);
    let py = Vec3::of(mesh.positions[cy.position as usize]);
    let pm = Vec3::of(mesh.positions[middle as usize]);
    let span = py - px;
    let s = ((pm - px).dot(span) / span.dot(span)).clamp(0.0, 1.0);
    let m = mesh.corners.len() as u32;
    mesh.corners.push(Corner {
        position: middle,
        uvq: lerp_uvq(cx.uvq, cy.uvq, s),
        back_uvq: lerp_uvq(cx.back_uvq, cy.back_uvq, s),
        normal: lerp_normal(cx.normal, cy.normal, s),
    });
    mesh.faces[face] = Face::Tri([x, m, d]);
    mesh.faces.push(Face::Tri([m, y, d]));
    let data = mesh.face_data[face];
    mesh.face_data.push(data);
    true
}

pub fn drop_degenerates(mesh: &mut Mesh, tolerance: Uu) -> Degenerates {
    let mut outcome = Degenerates::default();
    let mut stalled = false;
    loop {
        let count = mesh.faces.len();
        let degenerate: Vec<bool> = (0..count)
            .map(|face| is_degenerate(mesh, face, tolerance))
            .collect();
        if !degenerate.contains(&true) {
            return outcome;
        }
        let triangles: Vec<[u32; 3]> = (0..count)
            .map(|face| triangle_positions(mesh, face))
            .collect();
        let edges = Edges::build(&triangles);
        let mut touched = vec![false; count];
        let mut keep = vec![true; count];
        for face in (0..count).filter(|&f| degenerate[f]) {
            let Some((a, b, middle)) = long_edge_and_middle(mesh, face) else {
                keep[face] = false;
                outcome.collapsed += 1;
                continue;
            };
            let across: Vec<usize> = edges
                .around(a, b)
                .iter()
                .map(|i| i.face as usize)
                .filter(|&f| f != face)
                .collect();
            let waiting = across.iter().any(|&f| degenerate[f] || touched[f]);
            if waiting && !stalled {
                continue;
            }
            keep[face] = false;
            for f in across {
                if degenerate[f] || touched[f] {
                    continue;
                }
                if split(mesh, f, a, b, middle) {
                    touched[f] = true;
                    outcome.neighbours_split += 1;
                }
            }
        }
        keep.resize(mesh.faces.len(), true);
        let dropped = retain_faces(mesh, &keep);
        outcome.dropped += dropped;
        stalled = dropped == 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{indexed, position_triangles};
    use skp_core::mesh::{Point, DEFAULT_WELD_TOLERANCE};

    fn points() -> Vec<Point> {
        vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(10.0, 0.0, 0.0),
            Point::new(0.0, 10.0, 0.0),
            Point::new(5.0, 0.0, 0.0),
            Point::new(5.0, 0.001, 0.0),
            Point::new(5.0, 0.01, 0.0),
        ]
    }

    #[test]
    fn a_triangle_with_a_repeated_position_is_dropped() {
        let mut mesh = indexed(&points(), &[[0, 1, 2], [0, 1, 1]]);
        let outcome = drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!((outcome.dropped, outcome.collapsed), (1, 1));
        assert_eq!(mesh.faces.len(), 1);
    }

    #[test]
    fn a_lone_collinear_triangle_is_dropped() {
        let mut mesh = indexed(&points(), &[[0, 1, 3], [0, 2, 3]]);
        let outcome = drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!(outcome.dropped, 1);
        assert_eq!(outcome.neighbours_split, 0);
        assert_eq!(mesh.face_data.len(), 1);
    }

    #[test]
    fn a_sliver_thinner_than_the_tolerance_is_dropped() {
        let mut mesh = indexed(&points(), &[[0, 1, 4]]);
        assert_eq!(
            drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE).dropped,
            1
        );
    }

    #[test]
    fn a_thin_but_real_triangle_is_kept() {
        let mut mesh = indexed(&points(), &[[0, 1, 5]]);
        assert_eq!(
            drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE),
            Degenerates::default()
        );
    }

    #[test]
    fn a_run_of_collinear_needles_leaves_every_edge_on_the_line_shared() {
        let points = [
            Point::new(0.0, 0.0, 0.0),
            Point::new(3.0, 0.0, 0.0),
            Point::new(6.0, 0.0, 0.0),
            Point::new(10.0, 0.0, 0.0),
            Point::new(5.0, 5.0, 0.0),
            Point::new(5.0, -5.0, 0.0),
        ];
        let mut mesh = indexed(
            &points,
            &[
                [0, 1, 2],
                [0, 2, 3],
                [0, 3, 4],
                [1, 0, 5],
                [2, 1, 5],
                [3, 2, 5],
            ],
        );
        let outcome = drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!(outcome.dropped, 2);
        assert_eq!(outcome.neighbours_split, 2);
        assert_eq!(mesh.faces.len(), 6);
        let counts = Edges::build(&position_triangles(&mesh)).census();
        assert_eq!((counts.open, counts.non_manifold), (4, 0));
        let area: f64 = (0..mesh.faces.len())
            .map(|f| {
                let [a, b, c] = triangle_points(&mesh, f);
                crate::geometry::area_vector(a, b, c).z / 2.0
            })
            .sum();
        assert!((area - 50.0).abs() < 1e-9);
    }

    #[test]
    fn two_needles_across_each_other_still_terminate() {
        let mut mesh = indexed(&points(), &[[0, 3, 1], [0, 1, 3]]);
        let outcome = drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!(outcome.dropped, 2);
        assert!(mesh.faces.is_empty());
    }

    #[test]
    fn a_needle_splits_its_long_edge_neighbour_so_the_edge_stays_shared() {
        let mut points = points();
        points.push(Point::new(5.0, 5.0, 0.0));
        points.push(Point::new(5.0, -5.0, 0.0));
        let mut mesh = indexed(&points, &[[0, 3, 1], [0, 1, 6], [3, 0, 7], [1, 3, 7]]);
        for c in 3..6 {
            mesh.corners[c].uvq.u *= 3.0;
        }
        let outcome = drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!(
            outcome,
            Degenerates {
                dropped: 1,
                collapsed: 0,
                neighbours_split: 1
            }
        );
        assert_eq!(mesh.faces.len(), 4);
        let tris = position_triangles(&mesh);
        let edges = Edges::build(&tris);
        assert_eq!(edges.degree(0, 3), 2);
        assert_eq!(edges.degree(3, 1), 2);
        assert_eq!(edges.degree(0, 1), 0);
        let upper: Vec<Uvq> = (0..mesh.faces.len())
            .filter(|&f| tris[f].contains(&6))
            .flat_map(|f| mesh.faces[f].corners().to_vec())
            .map(|c| mesh.corners[c as usize])
            .filter(|c| c.position == 3)
            .map(|c| c.uvq)
            .collect();
        assert_eq!(upper.len(), 2);
        assert!(upper.iter().all(|uvq| (uvq.u - 15.0).abs() < 1e-12));
        assert_eq!(mesh.validate(), Ok(()));
        let area: f64 = (0..mesh.faces.len())
            .map(|f| {
                let [a, b, c] = triangle_points(&mesh, f);
                crate::geometry::area_vector(a, b, c).z / 2.0
            })
            .sum();
        assert!((area - 50.0).abs() < 1e-9);
    }
}
