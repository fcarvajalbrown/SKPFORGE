use crate::error::RetopoError;
use crate::surface::Surface;
use skp_core::correspondence::{Correspondence, CorrespondenceBuilder};
use skp_core::geometry::Vec3;
use skp_core::mesh::{Corner, Face, FaceData, Mesh};
use skp_core::progress::{CancelToken, Progress, ProgressSink};
use skp_core::topology::Incidence;
use skp_core::units::Uu;
use std::f64::consts::FRAC_PI_2;
use std::fmt;

const STAGES: u64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PairReport {
    pub high_triangles: usize,
    pub candidates: usize,
    pub quads: usize,
    pub triangles_left: usize,
}

#[derive(Debug, Clone)]
pub struct Paired {
    pub mesh: Mesh,
    pub correspondence: Correspondence,
    pub report: PairReport,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    score: f64,
    edge: (u32, u32),
    first: u32,
    second: u32,
    outline: ([u32; 4], u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Alone,
    Leads(usize),
    Follows,
}

struct Triangles {
    corners: Vec<[u32; 3]>,
    data: Vec<FaceData>,
}

impl Triangles {
    fn of(mesh: &Mesh) -> Triangles {
        let mut corners = Vec::with_capacity(mesh.triangle_count());
        let mut data = Vec::with_capacity(mesh.triangle_count());
        for (face, face_data) in mesh.faces.iter().zip(&mesh.face_data) {
            for tri in face.triangulate() {
                corners.push(tri);
                data.push(*face_data);
            }
        }
        Triangles { corners, data }
    }

    fn corner_at(&self, mesh: &Mesh, triangle: u32, position: u32) -> Option<u32> {
        self.corners[triangle as usize]
            .into_iter()
            .find(|&c| mesh.corners[c as usize].position == position)
    }
}

pub fn pair(
    mesh: &Mesh,
    coplanar_tolerance: Uu,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> Result<Paired, RetopoError> {
    mesh.validate()?;
    cancel.check()?;
    let surface = Surface::of(mesh);
    let triangles = Triangles::of(mesh);
    if surface.triangles.is_empty() {
        return Err(RetopoError::EmptyMesh);
    }
    step(1, cancel, progress)?;

    let mut candidates: Vec<Candidate> = surface
        .edges
        .iter()
        .filter_map(|(edge, incidences)| {
            candidate(
                mesh,
                &surface,
                &triangles,
                edge,
                incidences,
                coplanar_tolerance,
            )
        })
        .collect();
    candidates.sort_by(|a, b| a.score.total_cmp(&b.score).then(a.edge.cmp(&b.edge)));
    let count = surface.triangles.len();
    let mut role: Vec<Role> = vec![Role::Alone; count];
    let mut quads = Vec::new();
    for c in &candidates {
        if role[c.first as usize] == Role::Alone && role[c.second as usize] == Role::Alone {
            role[c.first as usize] = Role::Leads(quads.len());
            role[c.second as usize] = Role::Follows;
            quads.push(c.outline);
        }
    }
    step(2, cancel, progress)?;

    let mut builder = CorrespondenceBuilder::new(count);
    let mut faces = Vec::with_capacity(count - quads.len());
    let mut face_data = Vec::with_capacity(count - quads.len());
    let mut low = 0u32;
    for high in 0..count as u32 {
        match role[high as usize] {
            Role::Follows => {}
            Role::Leads(index) => {
                let (corners, second) = quads[index];
                faces.push(Face::Quad(corners));
                face_data.push(triangles.data[high as usize]);
                builder.push(low, high);
                builder.push(low + 1, second);
                low += 2;
            }
            Role::Alone => {
                faces.push(Face::Tri(triangles.corners[high as usize]));
                face_data.push(triangles.data[high as usize]);
                builder.push(low, high);
                low += 1;
            }
        }
    }
    let paired = Mesh {
        faces,
        face_data,
        ..mesh.clone()
    };
    paired.validate()?;
    let correspondence = builder.build();
    correspondence.validate()?;
    step(3, cancel, progress)?;

    Ok(Paired {
        report: PairReport {
            high_triangles: count,
            candidates: candidates.len(),
            quads: quads.len(),
            triangles_left: count - 2 * quads.len(),
        },
        mesh: paired,
        correspondence,
    })
}

fn step(done: u64, cancel: &CancelToken, progress: &dyn ProgressSink) -> Result<(), RetopoError> {
    progress.report(Progress::Measured {
        done,
        total: STAGES,
    });
    cancel.check()?;
    Ok(())
}

fn candidate(
    mesh: &Mesh,
    surface: &Surface,
    triangles: &Triangles,
    edge: (u32, u32),
    incidences: &[Incidence],
    tolerance: Uu,
) -> Option<Candidate> {
    let [a, b] = incidences else {
        return None;
    };
    if a.forward == b.forward {
        return None;
    }
    let (first, second) = if a.forward {
        (a.face, b.face)
    } else {
        (b.face, a.face)
    };
    if triangles.data[first as usize] != triangles.data[second as usize] {
        return None;
    }
    if !surface.is_coplanar(edge, incidences, tolerance.0) {
        return None;
    }
    for position in [edge.0, edge.1] {
        let here = triangles.corner_at(mesh, first, position)?;
        let there = triangles.corner_at(mesh, second, position)?;
        if !same_corner(&mesh.corners[here as usize], &mesh.corners[there as usize]) {
            return None;
        }
    }
    let normal = surface.normals[first as usize]?;
    let (p, q) = edge;
    let r = surface.far_vertex(first, edge)?;
    let s = surface.far_vertex(second, edge)?;
    let score = rectangularity([q, r, p, s].map(|v| surface.point(v)), normal)?;
    let corners = [
        triangles.corner_at(mesh, first, q)?,
        triangles.corner_at(mesh, first, r)?,
        triangles.corner_at(mesh, first, p)?,
        triangles.corner_at(mesh, second, s)?,
    ];
    Some(Candidate {
        score,
        edge,
        first,
        second,
        outline: (corners, second),
    })
}

fn same_corner(a: &Corner, b: &Corner) -> bool {
    a.uvq == b.uvq && a.back_uvq == b.back_uvq && a.normal == b.normal
}

fn rectangularity(outline: [Vec3; 4], normal: Vec3) -> Option<f64> {
    let mut score = 0.0;
    for i in 0..4 {
        let previous = outline[(i + 3) % 4] - outline[i];
        let next = outline[(i + 1) % 4] - outline[i];
        let turn = next.cross(previous);
        if turn.dot(normal) <= 0.0 {
            return None;
        }
        let angle = turn.length().atan2(next.dot(previous));
        score += (angle - FRAC_PI_2).abs();
    }
    Some(score)
}

impl fmt::Display for PairReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "high triangles         {}", self.high_triangles)?;
        writeln!(f, "pairable edges         {}", self.candidates)?;
        writeln!(f, "quads                  {}", self.quads)?;
        write!(f, "triangles left         {}", self.triangles_left)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skp_core::mesh::{MaterialId, Normal, Point, Uvq};
    use skp_core::progress::NoProgress;

    fn planar(points: &[Point], triangles: &[[u32; 3]], data: &[FaceData]) -> Mesh {
        let mut mesh = Mesh {
            positions: points.to_vec(),
            ..Mesh::default()
        };
        for (tri, face_data) in triangles.iter().zip(data) {
            let first = mesh.corners.len() as u32;
            for &position in tri {
                let p = points[position as usize];
                mesh.corners.push(Corner {
                    position,
                    uvq: Uvq {
                        u: p.x.0,
                        v: p.y.0,
                        q: 1.0,
                    },
                    back_uvq: Uvq::default(),
                    normal: Normal {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    },
                });
            }
            mesh.faces.push(Face::Tri([first, first + 1, first + 2]));
            mesh.face_data.push(*face_data);
        }
        mesh
    }

    fn square(far: Point) -> Mesh {
        planar(
            &[
                Point::new(0.0, 0.0, 0.0),
                Point::new(1.0, 0.0, 0.0),
                Point::new(1.0, 1.0, 0.0),
                far,
            ],
            &[[0, 1, 2], [0, 2, 3]],
            &[FaceData::default(); 2],
        )
    }

    fn run(mesh: &Mesh) -> Paired {
        pair(
            mesh,
            skp_core::mesh::DEFAULT_WELD_TOLERANCE,
            &CancelToken::new(),
            &NoProgress,
        )
        .unwrap()
    }

    fn positions(mesh: &Mesh, tri: [u32; 3]) -> [u32; 3] {
        tri.map(|c| mesh.corners[c as usize].position)
    }

    fn same_cycle(a: [u32; 3], b: [u32; 3]) -> bool {
        (0..3).any(|k| [a[k], a[(k + 1) % 3], a[(k + 2) % 3]] == b)
    }

    fn assert_exact(high: &Mesh, paired: &Paired) {
        let high_tris: Vec<[u32; 3]> = Triangles::of(high).corners;
        let low_tris: Vec<[u32; 3]> = Triangles::of(&paired.mesh).corners;
        assert_eq!(paired.correspondence.low_triangle_count(), low_tris.len());
        assert_eq!(paired.correspondence.validate(), Ok(()));
        for (low, &tri) in low_tris.iter().enumerate() {
            let mapped = paired.correspondence.high_for(low as u32);
            assert_eq!(mapped.len(), 1);
            let original = high_tris[mapped[0] as usize];
            assert!(same_cycle(
                positions(&paired.mesh, tri),
                positions(high, original)
            ));
        }
    }

    #[test]
    fn two_coplanar_triangles_become_one_quad_split_on_the_same_diagonal() {
        let mesh = square(Point::new(0.0, 1.0, 0.0));
        let paired = run(&mesh);
        assert_eq!(paired.report.quads, 1);
        assert_eq!(paired.report.triangles_left, 0);
        assert_eq!(paired.mesh.faces.len(), 1);
        assert!(paired.mesh.faces[0].is_quad());
        assert_exact(&mesh, &paired);
    }

    #[test]
    fn a_grid_pairs_completely_and_maps_one_to_one() {
        let mut points = Vec::new();
        for y in 0..3 {
            for x in 0..3 {
                points.push(Point::new(x as f64, y as f64, 0.0));
            }
        }
        let mut tris = Vec::new();
        for y in 0..2u32 {
            for x in 0..2u32 {
                let a = y * 3 + x;
                tris.push([a, a + 1, a + 4]);
                tris.push([a, a + 4, a + 3]);
            }
        }
        let mesh = planar(&points, &tris, &[FaceData::default(); 8]);
        let paired = run(&mesh);
        assert_eq!(paired.report.quads, 4);
        assert_eq!(paired.report.triangles_left, 0);
        assert_exact(&mesh, &paired);
        let again = run(&mesh);
        assert_eq!(again.mesh.faces, paired.mesh.faces);
        assert_eq!(again.correspondence, paired.correspondence);
    }

    #[test]
    fn different_materials_do_not_pair() {
        let mut mesh = square(Point::new(0.0, 1.0, 0.0));
        mesh.materials.push(skp_core::mesh::Material::default());
        mesh.face_data[1].front = Some(MaterialId(0));
        assert_eq!(run(&mesh).report.quads, 0);
    }

    #[test]
    fn a_uv_seam_on_the_shared_edge_does_not_pair() {
        let mut mesh = square(Point::new(0.0, 1.0, 0.0));
        mesh.corners[3].uvq.u += 0.5;
        let paired = run(&mesh);
        assert_eq!(paired.report.quads, 0);
        assert_exact(&mesh, &paired);
    }

    #[test]
    fn a_fold_does_not_pair() {
        assert_eq!(run(&square(Point::new(0.0, 1.0, 0.5))).report.quads, 0);
    }

    #[test]
    fn a_concave_outline_does_not_pair() {
        assert_eq!(run(&square(Point::new(1.5, 2.0, 0.0))).report.quads, 0);
    }

    #[test]
    fn inconsistent_winding_does_not_pair() {
        let mesh = planar(
            &[
                Point::new(0.0, 0.0, 0.0),
                Point::new(1.0, 0.0, 0.0),
                Point::new(1.0, 1.0, 0.0),
                Point::new(0.0, 1.0, 0.0),
            ],
            &[[0, 1, 2], [0, 3, 2]],
            &[FaceData::default(); 2],
        );
        assert_eq!(run(&mesh).report.quads, 0);
    }

    #[test]
    fn a_more_rectangular_pairing_wins() {
        let mesh = planar(
            &[
                Point::new(0.0, 0.0, 0.0),
                Point::new(1.0, 0.0, 0.0),
                Point::new(1.0, 1.0, 0.0),
                Point::new(0.0, 1.0, 0.0),
                Point::new(2.0, 1.2, 0.0),
            ],
            &[[0, 1, 2], [0, 2, 3], [1, 4, 2]],
            &[FaceData::default(); 3],
        );
        let paired = run(&mesh);
        assert_eq!(paired.report.quads, 1);
        assert_eq!(paired.report.candidates, 2);
        assert!(paired.mesh.faces[0].is_quad());
        assert_eq!(paired.correspondence.high_for(2), &[2]);
        assert_exact(&mesh, &paired);
    }

    #[test]
    fn a_cancelled_run_or_an_empty_mesh_is_refused() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let mesh = square(Point::new(0.0, 1.0, 0.0));
        let tolerance = skp_core::mesh::DEFAULT_WELD_TOLERANCE;
        assert_eq!(
            pair(&mesh, tolerance, &cancel, &NoProgress).unwrap_err(),
            RetopoError::Cancelled
        );
        assert_eq!(
            pair(&Mesh::new(), tolerance, &CancelToken::new(), &NoProgress).unwrap_err(),
            RetopoError::EmptyMesh
        );
    }
}
