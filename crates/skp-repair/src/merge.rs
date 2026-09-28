use crate::compact::retain_faces;
use skp_core::geometry::{area_vector, height, Vec3};
use skp_core::mesh::{Corner, Face, FaceData, Mesh, Uvq};
use skp_core::progress::{CancelToken, Cancelled};
use skp_core::units::Uu;
use std::collections::{BTreeSet, VecDeque};

const UV_EPSILON: f64 = 1e-6;
const NORMAL_EPSILON: f64 = 1e-6;

struct Merger<'a> {
    mesh: &'a mut Mesh,
    alive: Vec<bool>,
    incident: Vec<Vec<u32>>,
    points: Vec<Vec3>,
    tolerance: f64,
}

struct Fan {
    triangles: Vec<u32>,
    sub_fan: Vec<usize>,
    sub_fans: usize,
    targets: Vec<u32>,
}

fn same_uv(a: Uvq, b: Uvq) -> bool {
    match (a.project(), b.project()) {
        (Some((au, av)), Some((bu, bv))) => close(au, bu) && close(av, bv),
        _ => false,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= UV_EPSILON * a.abs().max(b.abs()).max(1.0)
}

fn same_attributes(a: &Corner, b: &Corner) -> bool {
    same_uv(a.uvq, b.uvq)
        && same_uv(a.back_uvq, b.back_uvq)
        && (a.normal.x - b.normal.x).abs() <= NORMAL_EPSILON
        && (a.normal.y - b.normal.y).abs() <= NORMAL_EPSILON
        && (a.normal.z - b.normal.z).abs() <= NORMAL_EPSILON
}

fn mergeable(a: &FaceData, b: &FaceData) -> bool {
    a.front == b.front
        && a.back == b.back
        && [
            a.q_variance,
            a.back_q_variance,
            b.q_variance,
            b.back_q_variance,
        ]
        .iter()
        .all(|&v| v == 0.0)
}

impl<'a> Merger<'a> {
    fn new(mesh: &'a mut Mesh, tolerance: Uu) -> Merger<'a> {
        let mut incident = vec![Vec::new(); mesh.positions.len()];
        for t in 0..mesh.faces.len() {
            for &c in mesh.faces[t].corners() {
                incident[mesh.corners[c as usize].position as usize].push(t as u32);
            }
        }
        let points = mesh.positions.iter().map(|&p| Vec3::of(p)).collect();
        Merger {
            alive: vec![true; mesh.faces.len()],
            mesh,
            incident,
            points,
            tolerance: tolerance.0,
        }
    }

    fn corners(&self, t: u32) -> [u32; 3] {
        let c = self.mesh.faces[t as usize].corners();
        [c[0], c[1], c[2]]
    }

    fn position(&self, corner: u32) -> u32 {
        self.mesh.corners[corner as usize].position
    }

    fn positions(&self, t: u32) -> [u32; 3] {
        self.corners(t).map(|c| self.position(c))
    }

    fn corner_at(&self, t: u32, p: u32) -> Option<u32> {
        self.corners(t).into_iter().find(|&c| self.position(c) == p)
    }

    fn normal_of(&self, tri: [u32; 3]) -> Option<Vec3> {
        let [a, b, c] = tri.map(|p| self.points[p as usize]);
        area_vector(a, b, c).normalised()
    }

    fn uv_at(&self, t: u32, x: Vec3, back: bool) -> Option<(f64, f64)> {
        let corners = self.corners(t).map(|c| self.mesh.corners[c as usize]);
        let uv = corners.map(|c| if back { c.back_uvq } else { c.uvq }.project());
        let [Some(uv0), Some(uv1), Some(uv2)] = uv else {
            return None;
        };
        let [p0, p1, p2] = corners.map(|c| self.points[c.position as usize]);
        let (e1, e2, d) = (p1 - p0, p2 - p0, x - p0);
        let (d00, d01, d11) = (e1.dot(e1), e1.dot(e2), e2.dot(e2));
        let (d20, d21) = (d.dot(e1), d.dot(e2));
        let denominator = d00 * d11 - d01 * d01;
        if denominator == 0.0 {
            return None;
        }
        let s = (d11 * d20 - d01 * d21) / denominator;
        let r = (d00 * d21 - d01 * d20) / denominator;
        Some((
            uv0.0 + s * (uv1.0 - uv0.0) + r * (uv2.0 - uv0.0),
            uv0.1 + s * (uv1.1 - uv0.1) + r * (uv2.1 - uv0.1),
        ))
    }

    fn smooth(&self, t: u32, u: u32, a: u32, b: u32) -> bool {
        if !mergeable(
            &self.mesh.face_data[t as usize],
            &self.mesh.face_data[u as usize],
        ) {
            return false;
        }
        let (pt, pu) = (self.positions(t), self.positions(u));
        let (Some(nt), Some(nu)) = (self.normal_of(pt), self.normal_of(pu)) else {
            return false;
        };
        if nt.dot(nu) <= 0.0 {
            return false;
        }
        let opposite = |tri: [u32; 3]| tri.into_iter().find(|&p| p != a && p != b);
        let (Some(ot), Some(ou)) = (opposite(pt), opposite(pu)) else {
            return false;
        };
        let (xt, xu) = (self.points[ot as usize], self.points[ou as usize]);
        if (xu - self.points[pt[0] as usize]).dot(nt).abs() > self.tolerance
            || (xt - self.points[pu[0] as usize]).dot(nu).abs() > self.tolerance
        {
            return false;
        }
        for p in [a, b] {
            let (Some(ct), Some(cu)) = (self.corner_at(t, p), self.corner_at(u, p)) else {
                return false;
            };
            if !same_attributes(
                &self.mesh.corners[ct as usize],
                &self.mesh.corners[cu as usize],
            ) {
                return false;
            }
        }
        let Some(cu) = self.corner_at(u, ou) else {
            return false;
        };
        let corner = self.mesh.corners[cu as usize];
        [(false, corner.uvq), (true, corner.back_uvq)]
            .into_iter()
            .all(
                |(back, uvq)| match (self.uv_at(t, xu, back), uvq.project()) {
                    (Some(e), Some(g)) => close(e.0, g.0) && close(e.1, g.1),
                    _ => false,
                },
            )
    }

    fn fan(&self, v: u32) -> Option<Fan> {
        let triangles: Vec<u32> = self.incident[v as usize].clone();
        if triangles.is_empty() {
            return None;
        }
        let mut neighbours: Vec<(u32, Vec<usize>)> = Vec::new();
        for (i, &t) in triangles.iter().enumerate() {
            for p in self.positions(t) {
                if p == v {
                    continue;
                }
                match neighbours.iter_mut().find(|(w, _)| *w == p) {
                    Some((_, list)) => list.push(i),
                    None => neighbours.push((p, vec![i])),
                }
            }
        }
        let mut boundary = 0;
        let mut sharp = Vec::new();
        let mut links: Vec<(usize, usize, bool)> = Vec::new();
        for (w, list) in &neighbours {
            match list.as_slice() {
                [_] => {
                    boundary += 1;
                    sharp.push(*w);
                }
                [i, j] => {
                    let smooth = self.smooth(triangles[*i], triangles[*j], v, *w);
                    if !smooth {
                        sharp.push(*w);
                    }
                    links.push((*i, *j, smooth));
                }
                _ => return None,
            }
        }
        if boundary != 0 && boundary != 2 {
            return None;
        }
        let expected = if boundary == 0 {
            neighbours.len()
        } else {
            neighbours.len() - 1
        };
        if triangles.len() != expected {
            return None;
        }
        let group = |smooth_only: bool| {
            let mut label = vec![usize::MAX; triangles.len()];
            let mut count = 0;
            for seed in 0..triangles.len() {
                if label[seed] != usize::MAX {
                    continue;
                }
                label[seed] = count;
                let mut stack = vec![seed];
                while let Some(i) = stack.pop() {
                    for &(a, b, smooth) in &links {
                        if smooth_only && !smooth {
                            continue;
                        }
                        let next = if a == i {
                            b
                        } else if b == i {
                            a
                        } else {
                            continue;
                        };
                        if label[next] == usize::MAX {
                            label[next] = count;
                            stack.push(next);
                        }
                    }
                }
                count += 1;
            }
            (label, count)
        };
        if group(false).1 != 1 {
            return None;
        }
        let (sub_fan, sub_fans) = group(true);
        let here = self.points[v as usize];
        let mut targets = match sharp.as_slice() {
            [] => neighbours.iter().map(|(w, _)| *w).collect(),
            &[w1, w2] => {
                let (a, b) = (self.points[w1 as usize], self.points[w2 as usize]);
                let line = b - a;
                let off_line =
                    (here - a).cross(line).length() / line.length().max(f64::MIN_POSITIVE);
                if off_line > self.tolerance || (a - here).dot(b - here) >= 0.0 {
                    return None;
                }
                vec![w1, w2]
            }
            _ => return None,
        };
        targets.sort_by(|&a, &b| {
            let d = |w: u32| (self.points[w as usize] - here).length();
            d(a).total_cmp(&d(b)).then(a.cmp(&b))
        });
        Some(Fan {
            triangles,
            sub_fan,
            sub_fans,
            targets,
        })
    }

    fn neighbours_of(&self, p: u32) -> BTreeSet<u32> {
        self.incident[p as usize]
            .iter()
            .flat_map(|&t| self.positions(t))
            .filter(|&q| q != p)
            .collect()
    }

    fn try_collapse(&mut self, v: u32, fan: &Fan, u: u32) -> bool {
        let dying: Vec<usize> = (0..fan.triangles.len())
            .filter(|&i| self.positions(fan.triangles[i]).contains(&u))
            .collect();
        let opposite: BTreeSet<u32> = dying
            .iter()
            .flat_map(|&i| self.positions(fan.triangles[i]))
            .filter(|&p| p != u && p != v)
            .collect();
        let common: BTreeSet<u32> = self
            .neighbours_of(v)
            .intersection(&self.neighbours_of(u))
            .copied()
            .collect();
        if common != opposite {
            return false;
        }
        let mut replacement = vec![None; fan.sub_fans];
        for &i in &dying {
            replacement[fan.sub_fan[i]] = self.corner_at(fan.triangles[i], u);
        }
        if replacement.iter().any(Option::is_none) {
            return false;
        }
        let here = self.points[v as usize];
        let mut edits = Vec::new();
        for (i, &t) in fan.triangles.iter().enumerate() {
            if dying.contains(&i) {
                continue;
            }
            let old = self.positions(t);
            let new = old.map(|p| if p == v { u } else { p });
            let (Some(n_old), Some(n_new)) = (self.normal_of(old), self.normal_of(new)) else {
                return false;
            };
            let [a, b, c] = new.map(|p| self.points[p as usize]);
            if n_old.dot(n_new) <= 0.0
                || height(a, b, c) < self.tolerance
                || (here - a).dot(n_new).abs() > self.tolerance
            {
                return false;
            }
            edits.push((t, replacement[fan.sub_fan[i]].unwrap_or_default()));
        }
        for (t, corner) in edits {
            let v_corner = self.corner_at(t, v);
            if let Face::Tri(cs) = &mut self.mesh.faces[t as usize] {
                for c in cs.iter_mut() {
                    if Some(*c) == v_corner {
                        *c = corner;
                    }
                }
            }
            self.incident[u as usize].push(t);
        }
        for &i in &dying {
            let t = fan.triangles[i];
            self.alive[t as usize] = false;
            for p in self.positions(t) {
                self.incident[p as usize].retain(|&x| x != t);
            }
        }
        self.incident[v as usize].clear();
        true
    }

    fn run(&mut self, cancel: &CancelToken) -> Result<usize, Cancelled> {
        let mut queue: VecDeque<u32> = (0..self.points.len() as u32).collect();
        let mut queued = vec![true; self.points.len()];
        let mut removed = 0;
        let mut steps = 0usize;
        while let Some(v) = queue.pop_front() {
            queued[v as usize] = false;
            steps += 1;
            if steps.is_multiple_of(4096) {
                cancel.check()?;
            }
            let Some(fan) = self.fan(v) else {
                continue;
            };
            let touched = self.neighbours_of(v);
            let collapsed = fan.targets.iter().any(|&u| self.try_collapse(v, &fan, u));
            if collapsed {
                removed += 1;
                for w in touched {
                    if !queued[w as usize] {
                        queued[w as usize] = true;
                        queue.push_back(w);
                    }
                }
            }
        }
        Ok(removed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Merging {
    pub vertices_removed: usize,
    pub triangles_removed: usize,
}

pub fn merge_coplanar(
    mesh: &mut Mesh,
    tolerance: Uu,
    cancel: &CancelToken,
) -> Result<Merging, Cancelled> {
    let mut merger = Merger::new(mesh, tolerance);
    let vertices_removed = merger.run(cancel)?;
    let alive = merger.alive;
    let triangles_removed = retain_faces(mesh, &alive);
    Ok(Merging {
        vertices_removed,
        triangles_removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::indexed;
    use skp_core::geometry::triangle_points;
    use skp_core::mesh::{Material, MaterialId, Point, DEFAULT_WELD_TOLERANCE};

    fn grid(nx: u32, ny: u32) -> Mesh {
        let mut points = Vec::new();
        for j in 0..=ny {
            for i in 0..=nx {
                points.push(Point::new(i as f64, j as f64, 0.0));
            }
        }
        let p = |i: u32, j: u32| j * (nx + 1) + i;
        let mut tris = Vec::new();
        for j in 0..ny {
            for i in 0..nx {
                tris.push([p(i, j), p(i + 1, j), p(i + 1, j + 1)]);
                tris.push([p(i, j), p(i + 1, j + 1), p(i, j + 1)]);
            }
        }
        indexed(&points, &tris)
    }

    fn run(mesh: &mut Mesh) -> Merging {
        merge_coplanar(mesh, DEFAULT_WELD_TOLERANCE, &CancelToken::new()).unwrap()
    }

    fn signed_areas(mesh: &Mesh) -> Vec<Vec3> {
        (0..mesh.faces.len())
            .map(|f| {
                let [a, b, c] = triangle_points(mesh, f);
                area_vector(a, b, c) * 0.5
            })
            .collect()
    }

    fn centroid_x(mesh: &Mesh, face: usize) -> f64 {
        let [a, b, c] = triangle_points(mesh, face);
        (a.x + b.x + c.x) / 3.0
    }

    fn assert_covers_flat(mesh: &Mesh, area: f64) {
        let areas = signed_areas(mesh);
        assert!(areas.iter().all(|a| a.z > 0.0));
        let total: f64 = areas.iter().map(|a| a.z).sum();
        assert!((total - area).abs() < 1e-9, "{total}");
        assert_eq!(mesh.validate(), Ok(()));
    }

    fn assert_uv_matches(mesh: &Mesh, expected: impl Fn(f64, Vec3) -> (f64, f64)) {
        for face in 0..mesh.faces.len() {
            let x = centroid_x(mesh, face);
            for &c in mesh.faces[face].corners() {
                let corner = mesh.corners[c as usize];
                let p = Vec3::of(mesh.positions[corner.position as usize]);
                let (u, v) = corner.uvq.project().unwrap();
                let (eu, ev) = expected(x, p);
                assert!((u - eu).abs() < 1e-9 && (v - ev).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn a_flat_grid_collapses_to_its_two_corner_triangles() {
        let mut mesh = grid(3, 3);
        let merging = run(&mut mesh);
        assert_eq!(mesh.faces.len(), 2);
        assert_eq!(merging.triangles_removed, 16);
        assert_eq!(merging.vertices_removed, 12);
        assert_covers_flat(&mesh, 9.0);
        assert_uv_matches(&mesh, |_, p| (p.x, p.y));
    }

    #[test]
    fn a_material_seam_is_kept_straight_and_each_side_keeps_its_material() {
        let mut mesh = grid(2, 2);
        mesh.materials = vec![Material::default(), Material::default()];
        for face in 0..mesh.faces.len() {
            let side = if centroid_x(&mesh, face) < 1.0 { 0 } else { 1 };
            mesh.face_data[face].front = Some(MaterialId(side));
        }
        run(&mut mesh);
        assert_eq!(mesh.faces.len(), 4);
        assert_covers_flat(&mesh, 4.0);
        for face in 0..mesh.faces.len() {
            let side = if centroid_x(&mesh, face) < 1.0 { 0 } else { 1 };
            assert_eq!(mesh.face_data[face].front, Some(MaterialId(side)));
        }
    }

    #[test]
    fn a_uv_seam_keeps_each_side_on_its_own_mapping() {
        let mut mesh = grid(2, 2);
        for face in 0..mesh.faces.len() {
            if centroid_x(&mesh, face) > 1.0 {
                for &c in mesh.faces[face].corners() {
                    mesh.corners[c as usize].uvq.u *= 2.0;
                }
            }
        }
        run(&mut mesh);
        assert_eq!(mesh.faces.len(), 4);
        assert_covers_flat(&mesh, 4.0);
        assert_uv_matches(&mesh, |x, p| {
            if x > 1.0 {
                (2.0 * p.x, p.y)
            } else {
                (p.x, p.y)
            }
        });
    }

    #[test]
    fn an_l_shaped_region_keeps_its_reflex_corner_and_never_folds() {
        let mut mesh = grid(2, 2);
        let keep: Vec<bool> = (0..mesh.faces.len())
            .map(|f| {
                let [a, b, c] = triangle_points(&mesh, f);
                !((a.x + b.x + c.x) / 3.0 > 1.0 && (a.y + b.y + c.y) / 3.0 > 1.0)
            })
            .collect();
        retain_faces(&mut mesh, &keep);
        run(&mut mesh);
        assert_covers_flat(&mesh, 3.0);
        assert_eq!(mesh.faces.len(), 4);
    }

    #[test]
    fn a_straight_crease_loses_its_middle_vertex_and_both_planes_stay_flat() {
        let points = [
            Point::new(0.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
            Point::new(0.0, 2.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(1.0, 1.0, 0.0),
            Point::new(1.0, 2.0, 0.0),
            Point::new(0.0, 0.0, 1.0),
            Point::new(0.0, 1.0, 1.0),
            Point::new(0.0, 2.0, 1.0),
        ];
        let mut mesh = indexed(
            &points,
            &[
                [0, 3, 4],
                [0, 4, 1],
                [1, 4, 5],
                [1, 5, 2],
                [0, 1, 7],
                [0, 7, 6],
                [1, 2, 8],
                [1, 8, 7],
            ],
        );
        let merging = run(&mut mesh);
        assert!(merging.vertices_removed >= 3);
        assert_eq!(mesh.faces.len(), 4);
        let areas = signed_areas(&mesh);
        let floor: f64 = areas.iter().map(|a| a.z).sum();
        let wall: f64 = areas.iter().map(|a| a.x).sum();
        assert!((floor - 2.0).abs() < 1e-9 && (wall - 2.0).abs() < 1e-9);
        assert!(areas.iter().all(|a| a.z >= 0.0 && a.x >= 0.0));
        assert_eq!(mesh.validate(), Ok(()));
    }

    #[test]
    fn projectively_distorted_faces_are_left_alone() {
        let mut mesh = grid(2, 2);
        for data in &mut mesh.face_data {
            data.q_variance = 1e-3;
        }
        assert_eq!(run(&mut mesh), Merging::default());
        assert_eq!(mesh.faces.len(), 8);
    }

    #[test]
    fn a_bent_surface_is_not_flattened() {
        let mut mesh = grid(2, 2);
        mesh.positions[4].z.0 = 0.1;
        let before = mesh.faces.len();
        run(&mut mesh);
        assert!(mesh.faces.len() >= before - 2);
        let has_raised =
            (0..mesh.faces.len()).any(|f| triangle_points(&mesh, f).iter().any(|p| p.z > 0.05));
        assert!(has_raised);
    }

    #[test]
    fn cancellation_is_honoured() {
        let mut mesh = grid(64, 64);
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            merge_coplanar(&mut mesh, DEFAULT_WELD_TOLERANCE, &cancel),
            Err(Cancelled)
        );
    }
}
