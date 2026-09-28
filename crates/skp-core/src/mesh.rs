use crate::units::{Uu, SKETCHUP_MERGE_DISTANCE};
use std::collections::HashMap;
use std::fmt;

pub const DEFAULT_WELD_TOLERANCE: Uu = SKETCHUP_MERGE_DISTANCE.to_uu();

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: Uu,
    pub y: Uu,
    pub z: Uu,
}

impl Point {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Point {
            x: Uu(x),
            y: Uu(y),
            z: Uu(z),
        }
    }

    pub fn distance_squared(self, other: Point) -> f64 {
        let dx = self.x.0 - other.x.0;
        let dy = self.y.0 - other.y.0;
        let dz = self.z.0 - other.z.0;
        dx * dx + dy * dy + dz * dz
    }

    fn weld_cell(self, tolerance: Uu) -> WeldCell {
        if tolerance.0 > 0.0 && tolerance.0.is_finite() {
            WeldCell::Grid([
                (self.x.0 / tolerance.0).floor() as i64,
                (self.y.0 / tolerance.0).floor() as i64,
                (self.z.0 / tolerance.0).floor() as i64,
            ])
        } else {
            WeldCell::Exact([self.x.0.to_bits(), self.y.0.to_bits(), self.z.0.to_bits()])
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WeldCell {
    Grid([i64; 3]),
    Exact([u64; 3]),
}

impl WeldCell {
    fn neighbourhood(self) -> Vec<WeldCell> {
        match self {
            WeldCell::Exact(_) => vec![self],
            WeldCell::Grid([x, y, z]) => {
                let mut cells = Vec::with_capacity(27);
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        for dz in -1..=1 {
                            cells.push(WeldCell::Grid([x + dx, y + dy, z + dz]));
                        }
                    }
                }
                cells
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Uvq {
    pub u: f64,
    pub v: f64,
    pub q: f64,
}

impl Uvq {
    pub fn project(self) -> Option<(f64, f64)> {
        if self.q == 0.0 || !self.q.is_finite() {
            return None;
        }
        Some((self.u / self.q, self.v / self.q))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Normal {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MaterialId(pub u32);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Material {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Corner {
    pub position: u32,
    pub uvq: Uvq,
    pub back_uvq: Uvq,
    pub normal: Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Tri([u32; 3]),
    Quad([u32; 4]),
}

impl Face {
    pub fn corners(&self) -> &[u32] {
        match self {
            Face::Tri(c) => c,
            Face::Quad(c) => c,
        }
    }

    pub fn is_quad(&self) -> bool {
        matches!(self, Face::Quad(_))
    }

    pub fn triangulate(&self) -> Vec<[u32; 3]> {
        match self {
            Face::Tri(c) => vec![*c],
            Face::Quad(c) => vec![[c[0], c[1], c[2]], [c[0], c[2], c[3]]],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FaceData {
    pub front: Option<MaterialId>,
    pub back: Option<MaterialId>,
    pub q_variance: f64,
}

impl FaceData {
    pub fn is_back_only(&self) -> bool {
        self.front.is_none() && self.back.is_some()
    }

    pub fn is_projectively_distorted(&self, threshold: f64) -> bool {
        self.q_variance > threshold
    }
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub positions: Vec<Point>,
    pub corners: Vec<Corner>,
    pub faces: Vec<Face>,
    pub face_data: Vec<FaceData>,
    pub materials: Vec<Material>,
}

impl Mesh {
    pub fn new() -> Self {
        Mesh::default()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.faces
            .iter()
            .map(|f| match f {
                Face::Tri(_) => 1,
                Face::Quad(_) => 2,
            })
            .sum()
    }

    pub fn quad_fraction(&self) -> f64 {
        if self.faces.is_empty() {
            return 0.0;
        }
        let quads = self.faces.iter().filter(|f| f.is_quad()).count();
        quads as f64 / self.faces.len() as f64
    }

    pub fn material(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0 as usize)
    }

    pub fn corner_position(&self, corner: u32) -> Option<Point> {
        let c = self.corners.get(corner as usize)?;
        self.positions.get(c.position as usize).copied()
    }

    pub fn weld_map(&self, tolerance: Uu) -> Vec<u32> {
        let limit = tolerance.0 * tolerance.0;
        let mut cells: HashMap<WeldCell, Vec<u32>> = HashMap::new();
        let mut representatives: Vec<Point> = Vec::new();
        let mut remap = Vec::with_capacity(self.positions.len());
        for &p in &self.positions {
            let cell = p.weld_cell(tolerance);
            let nearest = cell
                .neighbourhood()
                .iter()
                .filter_map(|c| cells.get(c))
                .flatten()
                .map(|&id| (representatives[id as usize].distance_squared(p), id))
                .filter(|&(d, _)| d <= limit)
                .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let target = match nearest {
                Some((_, id)) => id,
                None => {
                    let id = representatives.len() as u32;
                    representatives.push(p);
                    cells.entry(cell).or_default().push(id);
                    id
                }
            };
            remap.push(target);
        }
        remap
    }

    pub fn validate(&self) -> Result<(), MeshError> {
        if self.faces.len() != self.face_data.len() {
            return Err(MeshError::FaceDataLengthMismatch {
                faces: self.faces.len(),
                face_data: self.face_data.len(),
            });
        }
        for (index, data) in self.face_data.iter().enumerate() {
            for material in [data.front, data.back].into_iter().flatten() {
                if material.0 as usize >= self.materials.len() {
                    return Err(MeshError::MaterialOutOfRange {
                        face: index,
                        material: material.0,
                    });
                }
            }
        }
        for (index, face) in self.faces.iter().enumerate() {
            for &corner in face.corners() {
                let c = self
                    .corners
                    .get(corner as usize)
                    .ok_or(MeshError::CornerOutOfRange {
                        face: index,
                        corner,
                    })?;
                if c.position as usize >= self.positions.len() {
                    return Err(MeshError::PositionOutOfRange {
                        corner,
                        position: c.position,
                    });
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeshError {
    FaceDataLengthMismatch { faces: usize, face_data: usize },
    CornerOutOfRange { face: usize, corner: u32 },
    PositionOutOfRange { corner: u32, position: u32 },
    MaterialOutOfRange { face: usize, material: u32 },
}

impl fmt::Display for MeshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MeshError::FaceDataLengthMismatch { faces, face_data } => {
                write!(
                    f,
                    "face count {faces} does not match face data count {face_data}"
                )
            }
            MeshError::CornerOutOfRange { face, corner } => {
                write!(
                    f,
                    "face {face} refers to corner {corner}, which does not exist"
                )
            }
            MeshError::PositionOutOfRange { corner, position } => {
                write!(
                    f,
                    "corner {corner} refers to position {position}, which does not exist"
                )
            }
            MeshError::MaterialOutOfRange { face, material } => {
                write!(
                    f,
                    "face {face} refers to material {material}, which does not exist"
                )
            }
        }
    }
}

impl std::error::Error for MeshError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad_mesh() -> Mesh {
        Mesh {
            positions: vec![
                Point::new(0.0, 0.0, 0.0),
                Point::new(1.0, 0.0, 0.0),
                Point::new(1.0, 1.0, 0.0),
                Point::new(0.0, 1.0, 0.0),
            ],
            corners: (0..4)
                .map(|i| Corner {
                    position: i,
                    ..Corner::default()
                })
                .collect(),
            faces: vec![Face::Quad([0, 1, 2, 3])],
            face_data: vec![FaceData::default()],
            materials: Vec::new(),
        }
    }

    fn welded(points: &[Point], tolerance: Uu) -> Vec<u32> {
        Mesh {
            positions: points.to_vec(),
            ..Mesh::default()
        }
        .weld_map(tolerance)
    }

    #[test]
    fn default_weld_tolerance_is_sketchups_merge_distance() {
        assert!((DEFAULT_WELD_TOLERANCE.0 - 0.00254).abs() < 1e-15);
    }

    #[test]
    fn points_within_tolerance_weld() {
        let map = welded(
            &[
                Point::new(1.0, 2.0, 3.0),
                Point::new(1.0 + 0.0002, 2.0 - 0.0003, 3.0),
            ],
            DEFAULT_WELD_TOLERANCE,
        );
        assert_eq!(map, vec![0, 0]);
    }

    #[test]
    fn points_beyond_tolerance_stay_apart() {
        let map = welded(
            &[Point::new(1.0, 2.0, 3.0), Point::new(1.01, 2.0, 3.0)],
            DEFAULT_WELD_TOLERANCE,
        );
        assert_eq!(map, vec![0, 1]);
    }

    #[test]
    fn points_straddling_a_grid_line_still_weld() {
        let tol = Uu(1.0);
        let map = welded(
            &[Point::new(0.999, 0.0, 0.0), Point::new(1.001, 0.0, 0.0)],
            tol,
        );
        assert_eq!(map, vec![0, 0]);
    }

    #[test]
    fn a_chain_of_close_points_does_not_weld_end_to_end() {
        let tol = Uu(1.0);
        let map = welded(
            &[
                Point::new(0.0, 0.0, 0.0),
                Point::new(0.9, 0.0, 0.0),
                Point::new(1.8, 0.0, 0.0),
            ],
            tol,
        );
        assert_eq!(map, vec![0, 0, 1]);
    }

    #[test]
    fn a_zero_tolerance_welds_only_identical_points() {
        let map = welded(
            &[
                Point::new(1.0, 0.0, 0.0),
                Point::new(1.0, 0.0, 0.0),
                Point::new(1.0 + 1e-12, 0.0, 0.0),
            ],
            Uu(0.0),
        );
        assert_eq!(map, vec![0, 0, 1]);
    }

    #[test]
    fn weld_map_is_independent_of_order() {
        let tol = DEFAULT_WELD_TOLERANCE;
        let forward = Mesh {
            positions: vec![
                Point::new(0.0, 0.0, 0.0),
                Point::new(0.0, 0.0, 0.0),
                Point::new(5.0, 0.0, 0.0),
            ],
            ..Mesh::default()
        };
        let map = forward.weld_map(tol);
        assert_eq!(map[0], map[1]);
        assert_ne!(map[0], map[2]);
    }

    #[test]
    fn two_faces_at_one_position_keep_separate_uvs() {
        let mesh = Mesh {
            positions: vec![Point::new(0.0, 0.0, 0.0)],
            corners: vec![
                Corner {
                    position: 0,
                    uvq: Uvq {
                        u: 0.0,
                        v: 0.0,
                        q: 1.0,
                    },
                    ..Corner::default()
                },
                Corner {
                    position: 0,
                    uvq: Uvq {
                        u: 7.0,
                        v: 3.0,
                        q: 1.0,
                    },
                    ..Corner::default()
                },
            ],
            ..Mesh::default()
        };
        assert_eq!(mesh.corners[0].position, mesh.corners[1].position);
        assert_ne!(mesh.corners[0].uvq, mesh.corners[1].uvq);
    }

    #[test]
    fn uvq_divides_by_q() {
        let uvq = Uvq {
            u: 4.0,
            v: 6.0,
            q: 2.0,
        };
        assert_eq!(uvq.project(), Some((2.0, 3.0)));
    }

    #[test]
    fn uvq_with_zero_q_does_not_divide() {
        let uvq = Uvq {
            u: 4.0,
            v: 6.0,
            q: 0.0,
        };
        assert_eq!(uvq.project(), None);
    }

    #[test]
    fn quad_counts_as_two_triangles() {
        let mesh = quad_mesh();
        assert_eq!(mesh.face_count(), 1);
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.quad_fraction(), 1.0);
    }

    #[test]
    fn quad_triangulates_across_a_consistent_diagonal() {
        let face = Face::Quad([0, 1, 2, 3]);
        assert_eq!(face.triangulate(), vec![[0, 1, 2], [0, 2, 3]]);
    }

    #[test]
    fn back_only_face_is_detectable() {
        let data = FaceData {
            front: None,
            back: Some(MaterialId(4)),
            q_variance: 0.0,
        };
        assert!(data.is_back_only());
    }

    #[test]
    fn validate_accepts_a_well_formed_mesh() {
        assert_eq!(quad_mesh().validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_a_corner_out_of_range() {
        let mut mesh = quad_mesh();
        mesh.faces = vec![Face::Quad([0, 1, 2, 99])];
        assert_eq!(
            mesh.validate(),
            Err(MeshError::CornerOutOfRange {
                face: 0,
                corner: 99
            })
        );
    }

    #[test]
    fn errors_display_without_debug_formatting() {
        assert_eq!(
            MeshError::FaceDataLengthMismatch {
                faces: 3,
                face_data: 1
            }
            .to_string(),
            "face count 3 does not match face data count 1"
        );
        assert_eq!(
            MeshError::CornerOutOfRange { face: 2, corner: 8 }.to_string(),
            "face 2 refers to corner 8, which does not exist"
        );
        assert_eq!(
            MeshError::PositionOutOfRange {
                corner: 5,
                position: 40
            }
            .to_string(),
            "corner 5 refers to position 40, which does not exist"
        );
    }

    #[test]
    fn validate_rejects_face_data_of_the_wrong_length() {
        let mut mesh = quad_mesh();
        mesh.face_data.clear();
        assert_eq!(
            mesh.validate(),
            Err(MeshError::FaceDataLengthMismatch {
                faces: 1,
                face_data: 0
            })
        );
    }

    #[test]
    fn validate_rejects_a_material_out_of_range() {
        let mut mesh = quad_mesh();
        mesh.face_data[0].back = Some(MaterialId(2));
        mesh.materials = vec![Material {
            name: "Brick".into(),
        }];
        assert_eq!(
            mesh.validate(),
            Err(MeshError::MaterialOutOfRange {
                face: 0,
                material: 2
            })
        );
    }

    #[test]
    fn material_is_looked_up_by_id() {
        let mut mesh = quad_mesh();
        mesh.materials = vec![
            Material {
                name: "Brick".into(),
            },
            Material {
                name: "Glass".into(),
            },
        ];
        assert_eq!(
            mesh.material(MaterialId(1)).map(|m| m.name.as_str()),
            Some("Glass")
        );
        assert_eq!(mesh.material(MaterialId(2)), None);
        assert_eq!(mesh.validate(), Ok(()));
    }
}
