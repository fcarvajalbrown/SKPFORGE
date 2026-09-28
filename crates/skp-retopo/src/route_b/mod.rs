pub mod adjacency;
pub mod dedge;
pub mod dset;
pub mod field_math;
pub mod hierarchy;
pub mod orient;
pub mod pcg32;
pub mod position;
pub mod subdivide;

use adjacency::{uniform_adjacency, Adjacency};
use dedge::{dedge_next, dedge_prev, DirectedGraph, INVALID};
use field_math::fast_acos;
use hierarchy::{Hierarchy, Level, RCP_OVERFLOW};
use pcg32::Pcg32;
use skp_core::geometry::Vec3;
use subdivide::subdivide;

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
        let g = DirectedGraph::build(self.v.len(), &self.f);
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
}
