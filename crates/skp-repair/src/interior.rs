use crate::compact::retain_faces;
use crate::winding::WindingField;
use skp_core::geometry::{area_vector, triangle_points, triangle_positions, Vec3};
use skp_core::mesh::Mesh;
use skp_core::progress::{CancelToken, Cancelled};
use skp_core::topology::{triangle_edges, Edges};
use skp_core::units::Uu;

const INSIDE: f64 = 0.5;
const SAMPLE_OFFSET_IN_TOLERANCES: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Culling {
    pub patches: usize,
    pub triangles: usize,
}

struct Soup {
    positions: Vec<[u32; 3]>,
    points: Vec<[Vec3; 3]>,
    normals: Vec<Option<Vec3>>,
    edges: Edges,
}

impl Soup {
    fn of(mesh: &Mesh) -> Soup {
        let positions: Vec<[u32; 3]> = (0..mesh.faces.len())
            .map(|f| triangle_positions(mesh, f))
            .collect();
        let points: Vec<[Vec3; 3]> = (0..mesh.faces.len())
            .map(|f| triangle_points(mesh, f))
            .collect();
        let normals = points
            .iter()
            .map(|[a, b, c]| area_vector(*a, *b, *c).normalised())
            .collect();
        let edges = Edges::build(&positions);
        Soup {
            positions,
            points,
            normals,
            edges,
        }
    }

    fn opposite(&self, t: usize, a: u32, b: u32) -> Vec3 {
        let i = self.positions[t]
            .iter()
            .position(|&p| p != a && p != b)
            .unwrap_or(0);
        self.points[t][i]
    }

    fn coplanar(&self, t: usize, u: usize, a: u32, b: u32, tolerance: f64) -> bool {
        let (Some(nt), Some(nu)) = (self.normals[t], self.normals[u]) else {
            return false;
        };
        if nt.dot(nu) <= 0.0 {
            return false;
        }
        let off_t = (self.opposite(u, a, b) - self.points[t][0]).dot(nt).abs();
        let off_u = (self.opposite(t, a, b) - self.points[u][0]).dot(nu).abs();
        off_t <= tolerance && off_u <= tolerance
    }

    fn twins(&self, t: usize, u: usize) -> bool {
        let mut a = self.positions[t];
        let mut b = self.positions[u];
        a.sort_unstable();
        b.sort_unstable();
        a == b
    }

    fn patches(&self, tolerance: f64) -> (Vec<Vec<usize>>, Vec<usize>) {
        let mut patch_of = vec![usize::MAX; self.positions.len()];
        let mut patches = Vec::new();
        for seed in 0..self.positions.len() {
            if patch_of[seed] != usize::MAX {
                continue;
            }
            let id = patches.len();
            patch_of[seed] = id;
            let mut members = Vec::new();
            let mut stack = vec![seed];
            while let Some(t) = stack.pop() {
                members.push(t);
                for (a, b) in triangle_edges(self.positions[t]) {
                    let Some(other) = self.edges.other(a, b, t as u32) else {
                        continue;
                    };
                    let u = other.face as usize;
                    if patch_of[u] == usize::MAX && self.coplanar(t, u, a, b, tolerance) {
                        patch_of[u] = id;
                        stack.push(u);
                    }
                }
            }
            patches.push(members);
        }
        (patches, patch_of)
    }

    fn closed_shells(&self, excluded: &[bool]) -> Vec<bool> {
        let remaining = |a: u32, b: u32| -> Vec<usize> {
            self.edges
                .around(a, b)
                .iter()
                .map(|i| i.face as usize)
                .filter(|&f| !excluded[f])
                .collect()
        };
        let mut seen = vec![false; self.positions.len()];
        let mut solid = vec![false; self.positions.len()];
        for seed in 0..self.positions.len() {
            if excluded[seed] || seen[seed] {
                continue;
            }
            seen[seed] = true;
            let mut shell = Vec::new();
            let mut closed = true;
            let mut stack = vec![seed];
            while let Some(t) = stack.pop() {
                shell.push(t);
                for (a, b) in triangle_edges(self.positions[t]) {
                    match remaining(a, b).as_slice() {
                        [x, y] => {
                            let u = if *x == t { *y } else { *x };
                            if !seen[u] {
                                seen[u] = true;
                                stack.push(u);
                            }
                        }
                        _ => closed = false,
                    }
                }
            }
            if closed {
                for t in shell {
                    solid[t] = true;
                }
            }
        }
        solid
    }

    fn enclosed_by_other_faces(&self, patch: &[usize], patch_of: &[usize]) -> bool {
        let id = patch_of[patch[0]];
        patch.iter().all(|&t| {
            triangle_edges(self.positions[t]).iter().all(|&(a, b)| {
                let around = self.edges.around(a, b);
                match around.len() {
                    0 | 1 => false,
                    2 => {
                        let u = self.edges.other(a, b, t as u32).map(|i| i.face as usize);
                        u.is_some_and(|u| patch_of[u] == id || self.twins(t, u))
                    }
                    _ => true,
                }
            })
        })
    }
}

pub fn cull_interior(
    mesh: &mut Mesh,
    tolerance: Uu,
    cancel: &CancelToken,
) -> Result<Culling, Cancelled> {
    let soup = Soup::of(mesh);
    let (patches, patch_of) = soup.patches(tolerance.0);
    cancel.check()?;
    let candidates: Vec<&Vec<usize>> = patches
        .iter()
        .filter(|p| soup.enclosed_by_other_faces(p, &patch_of))
        .collect();
    if candidates.is_empty() {
        return Ok(Culling::default());
    }
    let mut is_candidate = vec![false; soup.positions.len()];
    for patch in &candidates {
        for &t in patch.iter() {
            is_candidate[t] = true;
        }
    }
    let solid = soup.closed_shells(&is_candidate);
    if !solid.contains(&true) {
        return Ok(Culling::default());
    }
    let field = WindingField::new(
        soup.points
            .iter()
            .zip(&solid)
            .filter(|(_, &s)| s)
            .map(|(p, _)| *p)
            .collect(),
    );
    let offset = SAMPLE_OFFSET_IN_TOLERANCES * tolerance.0.max(f64::EPSILON);
    let mut keep = vec![true; soup.positions.len()];
    let mut culling = Culling::default();
    for patch in candidates {
        cancel.check()?;
        let Some(&largest) = patch.iter().max_by(|&&a, &&b| {
            let area = |t: usize| {
                let [p, q, r] = soup.points[t];
                area_vector(p, q, r).length()
            };
            area(a).total_cmp(&area(b))
        }) else {
            continue;
        };
        let Some(normal) = soup.normals[largest] else {
            continue;
        };
        let [p, q, r] = soup.points[largest];
        let centre = (p + q + r) * (1.0 / 3.0);
        let front = field.at(centre + normal * offset);
        let back = field.at(centre - normal * offset);
        if front.abs() > INSIDE && back.abs() > INSIDE {
            culling.patches += 1;
            culling.triangles += patch.len();
            for &t in patch.iter() {
                keep[t] = false;
            }
        }
    }
    retain_faces(mesh, &keep);
    Ok(culling)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::indexed;
    use skp_core::mesh::{Point, DEFAULT_WELD_TOLERANCE};

    struct Builder {
        xs: Vec<f64>,
        side: f64,
        triangles: Vec<[u32; 3]>,
    }

    impl Builder {
        fn new(xs: &[f64], side: f64) -> Builder {
            Builder {
                xs: xs.to_vec(),
                side,
                triangles: Vec::new(),
            }
        }

        fn p(&self, x: usize, y: u32, z: u32) -> u32 {
            x as u32 * 4 + y * 2 + z
        }

        fn quad(&mut self, q: [u32; 4]) {
            self.triangles.push([q[0], q[1], q[2]]);
            self.triangles.push([q[0], q[2], q[3]]);
        }

        fn tube(&mut self) {
            for x in 0..self.xs.len() - 1 {
                let p = |y, z| self.p(x, y, z);
                let n = |y, z| self.p(x + 1, y, z);
                let quads = [
                    [p(0, 0), p(1, 0), n(1, 0), n(0, 0)],
                    [p(0, 1), n(0, 1), n(1, 1), p(1, 1)],
                    [p(0, 0), n(0, 0), n(0, 1), p(0, 1)],
                    [p(1, 0), p(1, 1), n(1, 1), n(1, 0)],
                ];
                for q in quads {
                    self.quad(q);
                }
            }
        }

        fn end_caps(&mut self) {
            let last = self.xs.len() - 1;
            self.quad([
                self.p(0, 0, 0),
                self.p(0, 0, 1),
                self.p(0, 1, 1),
                self.p(0, 1, 0),
            ]);
            self.quad([
                self.p(last, 0, 0),
                self.p(last, 1, 0),
                self.p(last, 1, 1),
                self.p(last, 0, 1),
            ]);
        }

        fn wall_at(&mut self, x: usize, reversed_twin: bool) {
            let q = [
                self.p(x, 0, 0),
                self.p(x, 1, 0),
                self.p(x, 1, 1),
                self.p(x, 0, 1),
            ];
            self.quad(q);
            if reversed_twin {
                self.quad([q[0], q[3], q[2], q[1]]);
            }
        }

        fn mesh(&self) -> Mesh {
            let points: Vec<Point> = self
                .xs
                .iter()
                .flat_map(|&x| {
                    [(0.0, 0.0), (0.0, 1.0), (1.0, 0.0), (1.0, 1.0)]
                        .map(|(y, z)| Point::new(x, y * self.side, z * self.side))
                })
                .collect();
            indexed(&points, &self.triangles)
        }
    }

    fn run(mesh: &mut Mesh) -> Culling {
        cull_interior(mesh, DEFAULT_WELD_TOLERANCE, &CancelToken::new()).unwrap()
    }

    #[test]
    fn a_closed_box_loses_nothing() {
        let mut b = Builder::new(&[0.0, 100.0], 100.0);
        b.tube();
        b.end_caps();
        let mut mesh = b.mesh();
        assert_eq!(run(&mut mesh), Culling::default());
        assert_eq!(mesh.faces.len(), 12);
    }

    #[test]
    fn a_partition_inside_a_closed_box_is_culled() {
        let mut b = Builder::new(&[0.0, 100.0, 200.0], 100.0);
        b.tube();
        b.end_caps();
        b.wall_at(1, false);
        let mut mesh = b.mesh();
        let culling = run(&mut mesh);
        assert_eq!(culling.triangles, 2);
        assert_eq!(culling.patches, 1);
        assert_eq!(mesh.faces.len(), 20);
    }

    #[test]
    fn the_touching_faces_of_two_solids_are_both_culled() {
        let mut b = Builder::new(&[0.0, 100.0, 200.0], 100.0);
        b.tube();
        b.end_caps();
        b.wall_at(1, true);
        let mut mesh = b.mesh();
        let culling = run(&mut mesh);
        assert_eq!(culling.triangles, 4);
        assert_eq!(mesh.faces.len(), 20);
    }

    #[test]
    fn a_partition_in_a_box_with_an_open_end_is_kept() {
        let mut b = Builder::new(&[0.0, 100.0, 200.0], 100.0);
        b.tube();
        b.wall_at(1, false);
        b.quad([b.p(0, 0, 0), b.p(0, 0, 1), b.p(0, 1, 1), b.p(0, 1, 0)]);
        let mut mesh = b.mesh();
        assert_eq!(run(&mut mesh), Culling::default());
        assert_eq!(mesh.faces.len(), 20);
    }

    #[test]
    fn a_pane_across_a_wall_opening_is_kept() {
        let mut b = Builder::new(&[0.0, 10.0, 20.0], 100.0);
        b.tube();
        b.wall_at(1, false);
        let mut mesh = b.mesh();
        assert_eq!(run(&mut mesh).triangles, 0);
        assert_eq!(mesh.faces.len(), 18);
    }

    #[test]
    fn a_two_sided_sheet_in_open_air_is_kept() {
        let mut b = Builder::new(&[0.0], 100.0);
        b.wall_at(0, true);
        let mut mesh = b.mesh();
        assert_eq!(run(&mut mesh).triangles, 0);
        assert_eq!(mesh.faces.len(), 4);
    }

    #[test]
    fn cancellation_is_honoured() {
        let mut b = Builder::new(&[0.0, 100.0, 200.0], 100.0);
        b.tube();
        b.end_caps();
        b.wall_at(1, false);
        let mut mesh = b.mesh();
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(
            cull_interior(&mut mesh, DEFAULT_WELD_TOLERANCE, &cancel),
            Err(Cancelled)
        );
    }
}
