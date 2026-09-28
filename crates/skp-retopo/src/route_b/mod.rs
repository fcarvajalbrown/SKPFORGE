pub mod adjacency;
pub mod correspond;
pub mod dedge;
pub mod dset;
pub mod extract;
pub mod field_math;
pub mod flip;
pub mod flow;
pub mod hierarchy;
pub mod integer;
pub mod orient;
pub mod pcg32;
pub mod position;
pub mod solve;
pub mod sparse;
pub mod subdivide;
pub mod valence;

use adjacency::{uniform_adjacency, Adjacency};
use dedge::{dedge_next, dedge_prev, split_non_manifold, DirectedGraph, INVALID};
use field_math::fast_acos;
use hierarchy::{Hierarchy, Level, RCP_OVERFLOW};
use pcg32::Pcg32;
use skp_core::geometry::Vec3;
use subdivide::{subdivide, subdivide_edge_diff};

use crate::error::RetopoError;
use correspond::geometric_correspondence;
use extract::{advanced_extract_quad, QuadMesh};
use flip::fix_flip_hierarchy;
use integer::{build_edge_info, build_integer_constraints, compute_max_flow, FlowReport};
use orient::{optimize_orientations, orientation_singularities};
use position::{optimize_positions, position_singularities};
use skp_core::correspondence::Correspondence;
use skp_core::mesh::{Corner, Face, FaceData, MaterialId, Mesh, Normal, Point};
use skp_core::progress::{CancelToken, Progress, ProgressSink};
use solve::{optimize_positions_dynamic, optimize_positions_fixed};
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};
use valence::fix_valence;

#[derive(Debug, Clone, Default)]
pub struct Parametrizer {
    pub v: Vec<Vec3>,
    pub f: Vec<[u32; 3]>,
    pub n: Vec<Vec3>,
    pub nf: Vec<Vec3>,
    pub a: Vec<f64>,
    pub rho: Vec<f64>,
    pub v2e: Vec<u32>,
    pub e2e: Vec<u32>,
    pub boundary: Vec<bool>,
    pub non_manifold: Vec<bool>,
    pub adj: Adjacency,
    pub sharp_edges: Vec<bool>,
    pub normalize_scale: f64,
    pub normalize_offset: Vec3,
    pub surface_area: f64,
    pub scale: f64,
    pub average_edge_length: f64,
    pub max_edge_length: f64,
    pub hierarchy: Hierarchy,
}

impl Parametrizer {
    pub fn load(positions: &[Vec3], triangles: &[[u32; 3]]) -> Parametrizer {
        let mut renumbered = vec![INVALID; positions.len()];
        let mut v = Vec::new();
        let f = triangles
            .iter()
            .map(|t| {
                t.map(|p| {
                    if renumbered[p as usize] == INVALID {
                        renumbered[p as usize] = v.len() as u32;
                        v.push(positions[p as usize]);
                    }
                    renumbered[p as usize]
                })
            })
            .collect();
        let mut field = Parametrizer {
            v,
            f,
            ..Parametrizer::default()
        };
        field.normalize_mesh();
        field
    }

    fn normalize_mesh(&mut self) {
        let mut max = [-1e30f64; 3];
        let mut min = [1e30f64; 3];
        for p in &self.v {
            for (j, c) in [p.x, p.y, p.z].into_iter().enumerate() {
                max[j] = max[j].max(c);
                min[j] = min[j].min(c);
            }
        }
        let scale = (max[0] - min[0]).max(max[1] - min[1]).max(max[2] - min[2]) * 0.5;
        let centre = [0, 1, 2].map(|j| (max[j] + min[j]) * 0.5);
        for p in &mut self.v {
            *p = Vec3::new(
                (p.x - centre[0]) / scale,
                (p.y - centre[1]) / scale,
                (p.z - centre[2]) / scale,
            );
        }
        self.normalize_scale = scale;
        self.normalize_offset = Vec3::new(
            0.5 * (max[0] + min[0]),
            0.5 * (max[1] + min[1]),
            0.5 * (max[2] + min[2]),
        );
    }

    pub fn compute_mesh_status(&mut self) {
        self.surface_area = 0.0;
        self.average_edge_length = 0.0;
        self.max_edge_length = 0.0;
        for face in &self.f {
            let v = face.map(|i| self.v[i as usize]);
            self.surface_area += 0.5 * (v[1] - v[0]).cross(v[2] - v[0]).length();
            for i in 0..3 {
                let len = (v[(i + 1) % 3] - v[i]).length();
                self.average_edge_length += len;
                if len > self.max_edge_length {
                    self.max_edge_length = len;
                }
            }
        }
        self.average_edge_length /= (self.f.len() * 3) as f64;
    }

    fn compute_direct_graph(&mut self) {
        let mut g = DirectedGraph::build(self.v.len(), &self.f);
        loop {
            let copies = split_non_manifold(&mut self.f, &g.e2e, self.v.len());
            if copies.is_empty() {
                break;
            }
            for original in copies {
                self.v.push(self.v[original as usize]);
                if !self.rho.is_empty() {
                    self.rho.push(self.rho[original as usize]);
                }
            }
            g = DirectedGraph::build(self.v.len(), &self.f);
        }
        self.v2e = g.v2e;
        self.e2e = g.e2e;
        self.boundary = g.boundary;
        self.non_manifold = g.non_manifold;
    }

    fn corner(&self, e: u32) -> Vec3 {
        self.v[self.f[(e / 3) as usize][(e % 3) as usize] as usize]
    }

    pub fn compute_smooth_normal(&mut self) {
        self.nf = self
            .f
            .iter()
            .map(|face| {
                let [v0, v1, v2] = face.map(|i| self.v[i as usize]);
                let n = (v1 - v0).cross(v2 - v0);
                let norm = n.length();
                if norm < RCP_OVERFLOW {
                    Vec3::new(1.0, 0.0, 0.0)
                } else {
                    n / norm
                }
            })
            .collect();

        self.n = (0..self.v.len())
            .map(|i| {
                let mut edge = self.v2e[i];
                if self.non_manifold[i] || edge == INVALID {
                    return Vec3::new(1.0, 0.0, 0.0);
                }
                let mut stop = edge;
                loop {
                    if self.sharp_edges[edge as usize] {
                        break;
                    }
                    edge = self.e2e[edge as usize];
                    if edge != INVALID {
                        edge = dedge_next(edge, 3);
                    }
                    if edge == stop || edge == INVALID {
                        break;
                    }
                }
                if edge == INVALID {
                    edge = stop;
                } else {
                    stop = edge;
                }
                let centre = self.v[i];
                let mut normal = Vec3::default();
                loop {
                    let base = edge - edge % 3;
                    let idx = edge % 3;
                    let d0 = self.corner(base + (idx + 1) % 3) - centre;
                    let d1 = self.corner(base + (idx + 2) % 3) - centre;
                    let angle = fast_acos(d0.dot(d1) / (d0.dot(d0) * d1.dot(d1)).sqrt());
                    if angle.is_finite() {
                        normal = normal + self.nf[(edge / 3) as usize] * angle;
                    }
                    let opp = self.e2e[edge as usize];
                    if opp == INVALID {
                        break;
                    }
                    edge = dedge_next(opp, 3);
                    if self.sharp_edges[edge as usize] || edge == stop {
                        break;
                    }
                }
                let norm = normal.length();
                if norm > RCP_OVERFLOW {
                    normal / norm
                } else {
                    Vec3::new(1.0, 0.0, 0.0)
                }
            })
            .collect();
    }

    pub fn compute_vertex_area(&mut self) {
        let third = (1.0f32 / 3.0f32) as f64;
        self.a = (0..self.v.len())
            .map(|i| {
                let (mut edge, stop) = (self.v2e[i], self.v2e[i]);
                if self.non_manifold[i] || edge == INVALID {
                    return 0.0;
                }
                let mut vertex_area = 0.0;
                loop {
                    let v = self.corner(edge);
                    let vn = self.corner(dedge_next(edge, 3));
                    let vp = self.corner(dedge_prev(edge, 3));
                    let face_center = (v + vp + vn) * third;
                    let prev = (v + vp) * 0.5;
                    let next = (v + vn) * 0.5;
                    vertex_area += 0.5
                        * ((v - prev).cross(v - face_center).length()
                            + (v - next).cross(v - face_center).length());
                    let opp = self.e2e[edge as usize];
                    if opp == INVALID {
                        break;
                    }
                    edge = dedge_next(opp, 3);
                    if edge == stop {
                        break;
                    }
                }
                vertex_area
            })
            .collect();
    }

    pub fn initialize(&mut self, faces: usize, rng: &mut Pcg32) {
        self.compute_mesh_status();
        self.rho = vec![1.0; self.v.len()];
        let count = if faces == 0 { self.v.len() } else { faces };
        self.scale = (self.surface_area / count as f64).sqrt();
        let target_len = (self.scale / 2.0).min(self.average_edge_length * 2.0);

        if target_len < self.max_edge_length {
            self.compute_direct_graph();
            let s = subdivide(
                std::mem::take(&mut self.f),
                std::mem::take(&mut self.v),
                std::mem::take(&mut self.rho),
                std::mem::take(&mut self.e2e),
                &self.non_manifold,
                target_len,
            );
            self.f = s.faces;
            self.v = s.positions;
            self.rho = s.rho;
        }

        self.compute_direct_graph();
        self.adj = uniform_adjacency(&self.f, &self.v2e, &self.e2e);

        for _ in 0..5 {
            self.rho = (0..self.rho.len())
                .map(|i| {
                    self.adj[i]
                        .iter()
                        .fold(self.rho[i], |r, link| r.min(self.rho[link.id as usize]))
                })
                .collect();
        }
        self.sharp_edges = vec![false; self.f.len() * 3];
        self.compute_smooth_normal();
        self.compute_vertex_area();

        let finest = Level {
            v: std::mem::take(&mut self.v),
            n: std::mem::take(&mut self.n),
            a: std::mem::take(&mut self.a),
            adj: std::mem::take(&mut self.adj),
            ..Level::default()
        };
        self.hierarchy = Hierarchy::build(finest, self.scale, rng);
        self.hierarchy.faces = std::mem::take(&mut self.f);
        self.hierarchy.e2e = std::mem::take(&mut self.e2e);
    }
}

const STAGE_NAMES: [&str; 10] = [
    "initialise",
    "orientation field",
    "position field",
    "integer offsets",
    "edge split and flips",
    "fixed solve",
    "quad extraction",
    "valence",
    "dynamic solve",
    "correspondence",
];

#[derive(Debug, Clone)]
pub struct RoutedB {
    pub mesh: Mesh,
    pub correspondence: Correspondence,
    pub target_triangles: usize,
    pub quads_asked: usize,
    pub flow: FlowReport,
    pub stage_times: Vec<(&'static str, Duration)>,
}

struct Stages<'a> {
    cancel: &'a CancelToken,
    progress: &'a dyn ProgressSink,
    started: Instant,
    times: Vec<(&'static str, Duration)>,
}

impl Stages<'_> {
    fn done(&mut self) -> Result<(), RetopoError> {
        let name = STAGE_NAMES[self.times.len()];
        self.times.push((name, self.started.elapsed()));
        self.started = Instant::now();
        self.progress.report(Progress::Measured {
            done: self.times.len() as u64,
            total: STAGE_NAMES.len() as u64,
        });
        self.cancel.check()?;
        Ok(())
    }
}

fn input_triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
    mesh.faces
        .iter()
        .flat_map(|f| f.triangulate())
        .map(|t| t.map(|c| mesh.corners[c as usize].position))
        .collect()
}

pub fn route_b(
    mesh: &Mesh,
    target: usize,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> Result<RoutedB, RetopoError> {
    let triangles = input_triangles(mesh);
    if triangles.is_empty() {
        return Err(RetopoError::EmptyMesh);
    }
    if target == 0 {
        return Err(RetopoError::ZeroTarget);
    }
    cancel.check()?;
    let mut stages = Stages {
        cancel,
        progress,
        started: Instant::now(),
        times: Vec::new(),
    };
    let positions: Vec<Vec3> = mesh.positions.iter().map(|&p| Vec3::of(p)).collect();
    let quads_asked = (target / 2).max(1);
    let mut p = Parametrizer::load(&positions, &triangles);
    p.initialize(quads_asked, &mut Pcg32::default());
    stages.done()?;

    optimize_orientations(&mut p.hierarchy);
    let singularities = orientation_singularities(&mut p.hierarchy);
    stages.done()?;

    optimize_positions(&mut p.hierarchy);
    let pos = position_singularities(&p.hierarchy);
    stages.done()?;

    let h = &p.hierarchy;
    let mut info = build_edge_info(&h.faces, &h.e2e, &pos, &singularities);
    let l = &h.levels[0];
    build_integer_constraints(
        &h.faces,
        &l.q,
        &l.n,
        &singularities,
        &mut info,
        &mut Pcg32::seeded(0, 1),
    );
    let flow = compute_max_flow(&mut info);
    stages.done()?;

    subdivide_edge_diff(&mut p, &mut info, 1)?;
    fix_flip_hierarchy(&mut info)?;
    subdivide_edge_diff(&mut p, &mut info, 1)?;
    stages.done()?;

    optimize_positions_fixed(&mut p.hierarchy, &info)?;
    stages.done()?;

    let mut quads = advanced_extract_quad(&p, &mut info)?.quads;
    stages.done()?;

    fix_valence(&mut quads);
    stages.done()?;

    optimize_positions_dynamic(&p, &info, &mut quads)?;
    stages.done()?;

    let low = low_mesh(&p, &quads, mesh);
    let low_triangles: Vec<[Vec3; 3]> = (0..low.faces.len())
        .flat_map(|f| {
            low.faces[f].triangulate().into_iter().map(|t| {
                t.map(|c| Vec3::of(low.positions[low.corners[c as usize].position as usize]))
            })
        })
        .collect();
    let high_triangles: Vec<[Vec3; 3]> = triangles
        .iter()
        .map(|t| t.map(|v| positions[v as usize]))
        .collect();
    let correspondence = geometric_correspondence(&low_triangles, &high_triangles)?;
    let mesh_out = with_materials(low, &correspondence, mesh);
    mesh_out.validate()?;
    stages.done()?;

    Ok(RoutedB {
        mesh: mesh_out,
        correspondence,
        target_triangles: target,
        quads_asked,
        flow,
        stage_times: stages.times,
    })
}

fn low_mesh(p: &Parametrizer, quads: &QuadMesh, high: &Mesh) -> Mesh {
    let positions = quads
        .o
        .iter()
        .map(|&o| {
            let t = o * p.normalize_scale + p.normalize_offset;
            Point::new(t.x, t.y, t.z)
        })
        .collect();
    let mut corners = Vec::with_capacity(quads.faces.len() * 4);
    let mut faces = Vec::with_capacity(quads.faces.len());
    for f in &quads.faces {
        let base = corners.len() as u32;
        for &v in f {
            let n = quads.n[v as usize];
            corners.push(Corner {
                position: v,
                normal: Normal {
                    x: n.x,
                    y: n.y,
                    z: n.z,
                },
                ..Corner::default()
            });
        }
        faces.push(Face::Quad([base, base + 1, base + 2, base + 3]));
    }
    Mesh {
        positions,
        corners,
        face_data: vec![FaceData::default(); faces.len()],
        faces,
        materials: high.materials.clone(),
    }
}

type Sides = (Option<MaterialId>, Option<MaterialId>);

fn with_materials(mut low: Mesh, map: &Correspondence, high: &Mesh) -> Mesh {
    let mut high_face = Vec::new();
    for (f, face) in high.faces.iter().enumerate() {
        for _ in face.triangulate() {
            high_face.push(f);
        }
    }
    for f in 0..low.faces.len() {
        let mut votes: BTreeMap<Sides, usize> = BTreeMap::new();
        for low_triangle in [2 * f as u32, 2 * f as u32 + 1] {
            for &h in map.high_for(low_triangle) {
                let data = high.face_data[high_face[h as usize]];
                *votes.entry((data.front, data.back)).or_insert(0) += 1;
            }
        }
        let mut best: Option<(Sides, usize)> = None;
        for (&key, &count) in &votes {
            if best.is_none_or(|(_, c)| count > c) {
                best = Some((key, count));
            }
        }
        if let Some(((front, back), _)) = best {
            low.face_data[f] = FaceData {
                front,
                back,
                ..FaceData::default()
            };
        }
    }
    low
}

impl RoutedB {
    pub fn triangle_count(&self) -> usize {
        self.mesh.triangle_count()
    }
}

impl fmt::Display for RoutedB {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "quads asked            {}", self.quads_asked)?;
        writeln!(f, "quads made             {}", self.mesh.faces.len())?;
        writeln!(f, "low vertices           {}", self.mesh.positions.len())?;
        writeln!(f, "low triangles          {}", self.triangle_count())?;
        writeln!(
            f,
            "integer flow           {} of {} in {} round(s){}",
            self.flow.flow,
            self.flow.supply,
            self.flow.rounds,
            if self.flow.full { "" } else { ", not full" }
        )?;
        writeln!(
            f,
            "correspondence pairs   {}",
            self.correspondence.pair_count()
        )?;
        for (name, time) in &self.stage_times {
            writeln!(f, "  {name:<21}{:.2} s", time.as_secs_f64())?;
        }
        let over = self.triangle_count().abs_diff(self.target_triangles);
        write!(
            f,
            "budget                 {} triangles from {}",
            over, self.target_triangles
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(side: u32, size: f64) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let step = size / side as f64;
        let id = |x: u32, y: u32| y * (side + 1) + x;
        let positions = (0..=side)
            .flat_map(|y| (0..=side).map(move |x| Vec3::new(x as f64 * step, y as f64 * step, 0.0)))
            .collect();
        let mut faces = Vec::new();
        for y in 0..side {
            for x in 0..side {
                faces.push([id(x, y), id(x + 1, y), id(x + 1, y + 1)]);
                faces.push([id(x, y), id(x + 1, y + 1), id(x, y + 1)]);
            }
        }
        (positions, faces)
    }

    #[test]
    fn load_numbers_vertices_by_first_use_and_drops_unused_ones() {
        let positions = [
            Vec3::new(9.0, 9.0, 9.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(4.0, 0.0, 0.0),
            Vec3::new(0.0, 2.0, 0.0),
        ];
        let p = Parametrizer::load(&positions, &[[2, 3, 1]]);
        assert_eq!(p.f, [[0, 1, 2]]);
        assert_eq!(p.v.len(), 3);
        assert_eq!(p.v[0], Vec3::new(1.0, -0.5, 0.0));
        assert_eq!(p.v[2], Vec3::new(-1.0, -0.5, 0.0));
        assert_eq!(p.normalize_scale, 2.0);
        assert_eq!(p.normalize_offset, Vec3::new(2.0, 1.0, 0.0));
    }

    #[test]
    fn mesh_status_measures_area_and_edges() {
        let (v, f) = grid(2, 2.0);
        let mut p = Parametrizer::load(&v, &f);
        p.compute_mesh_status();
        assert_eq!(p.surface_area, 4.0);
        assert_eq!(p.max_edge_length, 2f64.sqrt());
    }

    #[test]
    fn a_flat_patch_has_the_plane_normal_and_areas_that_sum_to_its_surface() {
        let (v, f) = grid(4, 3.0);
        let mut p = Parametrizer::load(&v, &f);
        p.compute_mesh_status();
        p.compute_direct_graph();
        p.sharp_edges = vec![false; p.f.len() * 3];
        p.compute_smooth_normal();
        p.compute_vertex_area();
        assert!(p.n.iter().all(|n| (n.z - 1.0).abs() < 1e-12));
        let total: f64 = p.a.iter().sum();
        assert!((total - p.surface_area).abs() < 1e-6 * p.surface_area);
    }

    #[test]
    fn a_ridge_vertex_normal_bisects_the_two_faces() {
        let v = [
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 1.0),
        ];
        let mut p = Parametrizer::load(&v, &[[0, 2, 1], [0, 1, 3]]);
        p.compute_direct_graph();
        p.sharp_edges = vec![false; 6];
        p.compute_smooth_normal();
        let n = p.n[0];
        assert!(n.x.abs() < 1e-12 && n.y.abs() < 1e-12 && n.z > 0.0);
    }

    #[test]
    fn initialize_subdivides_to_the_target_and_builds_the_hierarchy() {
        let (v, f) = grid(3, 3.0);
        let mut p = Parametrizer::load(&v, &f);
        p.initialize(200, &mut Pcg32::default());
        let h = &p.hierarchy;
        assert!(h.faces.len() > 18);
        assert_eq!(h.e2e.len(), h.faces.len() * 3);
        assert_eq!(h.levels[0].v.len(), h.levels[0].n.len());
        assert_eq!(h.levels.last().map(|l| l.v.len()), Some(1));
        assert!((p.scale - (p.surface_area / 200.0).sqrt()).abs() < 1e-15);
        assert!(p.rho.iter().all(|&r| r <= 1.0));
    }

    fn cube_mesh(size: f64) -> Mesh {
        let (v, f) = crate::route_b::orient::tests::cube();
        let mut mesh = Mesh {
            positions: v
                .iter()
                .map(|p| Point::new(p.x * size, p.y * size, p.z * size))
                .collect(),
            materials: vec![
                skp_core::mesh::Material {
                    name: "bottom".into(),
                },
                skp_core::mesh::Material {
                    name: "rest".into(),
                },
            ],
            ..Mesh::default()
        };
        for (i, t) in f.iter().enumerate() {
            let base = mesh.corners.len() as u32;
            for &p in t {
                mesh.corners.push(Corner {
                    position: p,
                    ..Corner::default()
                });
            }
            mesh.faces.push(Face::Tri([base, base + 1, base + 2]));
            let material = if i < 2 { MaterialId(0) } else { MaterialId(1) };
            mesh.face_data.push(FaceData {
                front: Some(material),
                ..FaceData::default()
            });
        }
        mesh
    }

    #[test]
    fn route_b_turns_a_cube_into_quads_near_the_budget_with_a_valid_map() {
        let mesh = cube_mesh(100.0);
        let done = route_b(
            &mesh,
            600,
            &CancelToken::new(),
            &skp_core::progress::NoProgress,
        )
        .unwrap();
        assert!(done.mesh.faces.iter().all(|f| f.is_quad()));
        let tris = done.triangle_count();
        assert!((540..=660).contains(&tris), "{tris} triangles");
        assert_eq!(done.correspondence.low_triangle_count(), tris);
        done.correspondence.validate().unwrap();
        assert!(done.correspondence.pair_count() >= mesh.triangle_count());
        assert_eq!(done.stage_times.len(), STAGE_NAMES.len());
        for p in &done.mesh.positions {
            for c in [p.x.0, p.y.0, p.z.0] {
                assert!((-5.0..=105.0).contains(&c), "{p:?}");
            }
        }
    }

    #[test]
    fn route_b_quads_on_the_bottom_face_take_its_material() {
        let mesh = cube_mesh(100.0);
        let done = route_b(
            &mesh,
            600,
            &CancelToken::new(),
            &skp_core::progress::NoProgress,
        )
        .unwrap();
        let mut bottom = 0;
        for (f, face) in done.mesh.faces.iter().enumerate() {
            let zs: Vec<f64> = face
                .corners()
                .iter()
                .map(|&c| {
                    done.mesh.positions[done.mesh.corners[c as usize].position as usize]
                        .z
                        .0
                })
                .collect();
            if zs.iter().all(|z| z.abs() < 1.0) {
                bottom += 1;
                assert_eq!(done.mesh.face_data[f].front, Some(MaterialId(0)));
            }
        }
        assert!(bottom > 0);
    }

    #[test]
    fn a_cancelled_token_stops_route_b() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let got = route_b(
            &cube_mesh(1.0),
            600,
            &cancel,
            &skp_core::progress::NoProgress,
        );
        assert!(matches!(got, Err(RetopoError::Cancelled)));
    }
}
