use crate::compact::retain_faces;
use skp_core::geometry::{height, triangle_points, triangle_positions, Vec3};
use skp_core::mesh::{Corner, Face, Mesh, Normal, Uvq};
use skp_core::topology::triangle_edges;
use skp_core::units::Uu;
use std::collections::{HashMap, VecDeque};

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

fn split(
    mesh: &mut Mesh,
    face: usize,
    (a, b, middle): (u32, u32, u32),
    tolerance: Uu,
) -> Option<usize> {
    let c = mesh.faces[face].corners();
    let corners = [c[0], c[1], c[2]];
    let positions = corners.map(|k| mesh.corners[k as usize].position);
    let k = (0..3).find(|&k| {
        let (from, to) = (positions[k], positions[(k + 1) % 3]);
        (from == a && to == b) || (from == b && to == a)
    })?;
    let (x, y, d) = (corners[k], corners[(k + 1) % 3], corners[(k + 2) % 3]);
    let (cx, cy) = (mesh.corners[x as usize], mesh.corners[y as usize]);
    let px = Vec3::of(mesh.positions[cx.position as usize]);
    let py = Vec3::of(mesh.positions[cy.position as usize]);
    let pm = Vec3::of(mesh.positions[middle as usize]);
    let pd = Vec3::of(mesh.positions[mesh.corners[d as usize].position as usize]);
    if height(px, pm, pd) < tolerance.0 || height(pm, py, pd) < tolerance.0 {
        return None;
    }
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
    Some(mesh.faces.len() - 1)
}

struct FaceEdges(HashMap<(u32, u32), Vec<usize>>);

impl FaceEdges {
    fn key(a: u32, b: u32) -> (u32, u32) {
        (a.min(b), a.max(b))
    }

    fn add(&mut self, mesh: &Mesh, face: usize) {
        for (a, b) in triangle_edges(triangle_positions(mesh, face)) {
            self.0.entry(Self::key(a, b)).or_default().push(face);
        }
    }

    fn remove(&mut self, mesh: &Mesh, face: usize) {
        for (a, b) in triangle_edges(triangle_positions(mesh, face)) {
            if let Some(faces) = self.0.get_mut(&Self::key(a, b)) {
                faces.retain(|&f| f != face);
            }
        }
    }

    fn across(&self, a: u32, b: u32, face: usize) -> Vec<usize> {
        self.0
            .get(&Self::key(a, b))
            .map(|faces| faces.iter().copied().filter(|&f| f != face).collect())
            .unwrap_or_default()
    }
}

pub fn drop_degenerates(mesh: &mut Mesh, tolerance: Uu) -> Degenerates {
    let mut outcome = Degenerates::default();
    let mut edges = FaceEdges(HashMap::new());
    for face in 0..mesh.faces.len() {
        edges.add(mesh, face);
    }
    let mut alive = vec![true; mesh.faces.len()];
    let mut queue: VecDeque<usize> = (0..mesh.faces.len())
        .filter(|&f| is_degenerate(mesh, f, tolerance))
        .collect();
    let mut deferred_in_a_row = 0;
    let mut forcing = 0;
    while let Some(face) = queue.pop_front() {
        if !alive[face] {
            continue;
        }
        let Some((a, b, middle)) = long_edge_and_middle(mesh, face) else {
            edges.remove(mesh, face);
            alive[face] = false;
            outcome.collapsed += 1;
            outcome.dropped += 1;
            deferred_in_a_row = 0;
            continue;
        };
        let across = edges.across(a, b, face);
        let waiting = across.iter().any(|&f| is_degenerate(mesh, f, tolerance));
        if forcing > 0 {
            forcing -= 1;
        } else if waiting {
            if deferred_in_a_row <= queue.len() {
                queue.push_back(face);
                deferred_in_a_row += 1;
                continue;
            }
            forcing = queue.len();
        }
        deferred_in_a_row = 0;
        for f in across {
            if is_degenerate(mesh, f, tolerance) {
                continue;
            }
            edges.remove(mesh, f);
            let added = split(mesh, f, (a, b, middle), tolerance);
            edges.add(mesh, f);
            if let Some(new) = added {
                alive.push(true);
                edges.add(mesh, new);
                outcome.neighbours_split += 1;
            }
        }
        edges.remove(mesh, face);
        alive[face] = false;
        outcome.dropped += 1;
    }
    retain_faces(mesh, &alive);
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{indexed, position_triangles};
    use skp_core::mesh::{Point, DEFAULT_WELD_TOLERANCE};
    use skp_core::topology::Edges;

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
                skp_core::geometry::area_vector(a, b, c).z / 2.0
            })
            .sum();
        assert!((area - 50.0).abs() < 1e-9);
    }

    #[test]
    fn a_neighbour_too_thin_to_split_cleanly_is_left_whole() {
        let points = [
            Point::new(0.0, 0.0, 0.0),
            Point::new(10.0, 0.0, 0.0),
            Point::new(5.0, 0.0, 0.0),
            Point::new(0.0, 0.003, 0.0),
        ];
        let mut mesh = indexed(&points, &[[0, 2, 1], [0, 1, 3]]);
        let outcome = drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!((outcome.dropped, outcome.neighbours_split), (1, 0));
        assert_eq!(mesh.faces.len(), 1);
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
                skp_core::geometry::area_vector(a, b, c).z / 2.0
            })
            .sum();
        assert!((area - 50.0).abs() < 1e-9);
    }
}
