use skp_core::mesh::{Material, MaterialId, Uvq};
use skp_core::units::Inches;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform(pub [f64; 16]);

impl Transform {
    pub const IDENTITY: Transform = Transform([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);

    pub fn translation(x: f64, y: f64, z: f64) -> Self {
        let mut m = Transform::IDENTITY;
        m.0[12] = x;
        m.0[13] = y;
        m.0[14] = z;
        m
    }

    pub fn scale(x: f64, y: f64, z: f64) -> Self {
        let mut m = Transform::IDENTITY;
        m.0[0] = x;
        m.0[5] = y;
        m.0[10] = z;
        m
    }

    fn at(&self, row: usize, col: usize) -> f64 {
        self.0[col * 4 + row]
    }

    pub fn then_apply(&self, child: &Transform) -> Transform {
        let mut out = [0.0; 16];
        for col in 0..4 {
            for row in 0..4 {
                out[col * 4 + row] = (0..4).map(|k| self.at(row, k) * child.at(k, col)).sum();
            }
        }
        Transform(out)
    }

    pub fn apply_point(&self, p: [f64; 3]) -> Option<[f64; 3]> {
        let row = |r: usize| {
            self.at(r, 0) * p[0] + self.at(r, 1) * p[1] + self.at(r, 2) * p[2] + self.at(r, 3)
        };
        let w = row(3);
        if w == 0.0 || !w.is_finite() {
            return None;
        }
        Some([row(0) / w, row(1) / w, row(2) / w])
    }

    fn cofactors(&self) -> [[f64; 3]; 3] {
        let a = |r: usize, c: usize| self.at(r, c);
        let mut c = [[0.0; 3]; 3];
        for (i, row) in c.iter_mut().enumerate() {
            for (j, cell) in row.iter_mut().enumerate() {
                let (r0, r1) = others(i);
                let (c0, c1) = others(j);
                let minor = a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0);
                *cell = if (i + j) % 2 == 0 { minor } else { -minor };
            }
        }
        c
    }

    pub fn linear_determinant(&self) -> f64 {
        let c = self.cofactors();
        (0..3).map(|j| self.at(0, j) * c[0][j]).sum()
    }

    pub fn is_mirroring(&self) -> bool {
        self.linear_determinant() < 0.0
    }

    pub fn apply_normal(&self, n: [f64; 3]) -> [f64; 3] {
        let c = self.cofactors();
        let sign = self.linear_determinant().signum();
        let mut out = [0.0; 3];
        for (i, value) in out.iter_mut().enumerate() {
            *value = sign * (c[i][0] * n[0] + c[i][1] * n[1] + c[i][2] * n[2]);
        }
        let length = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt();
        if length == 0.0 || !length.is_finite() {
            return [0.0; 3];
        }
        [out[0] / length, out[1] / length, out[2] / length]
    }
}

fn others(i: usize) -> (usize, usize) {
    match i {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

impl Default for Transform {
    fn default() -> Self {
        Transform::IDENTITY
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SourceVertex {
    pub position: [Inches; 3],
    pub normal: [f64; 3],
    pub front: Uvq,
    pub back: Uvq,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceFace {
    pub vertices: Vec<SourceVertex>,
    pub triangles: Vec<[u32; 3]>,
    pub front: Option<MaterialId>,
    pub back: Option<MaterialId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NodeKind {
    #[default]
    Root,
    Group,
    Instance,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    pub kind: NodeKind,
    pub transform: Transform,
    pub material: Option<MaterialId>,
    pub faces: Vec<SourceFace>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn face_count(&self) -> usize {
        self.faces.len() + self.children.iter().map(Node::face_count).sum::<usize>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoadStatus {
    #[default]
    Current,
    NewerThanSdk,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelUnits {
    #[default]
    Inches,
    Feet,
    Millimetres,
    Centimetres,
    Metres,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    pub materials: Vec<Material>,
    pub root: Node,
    pub load_status: LoadStatus,
    pub units: ModelUnits,
    pub hidden_skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-12)
    }

    #[test]
    fn translation_moves_a_point() {
        let t = Transform::translation(1.0, 2.0, 3.0);
        assert_eq!(t.apply_point([1.0, 1.0, 1.0]), Some([2.0, 3.0, 4.0]));
    }

    #[test]
    fn composition_applies_the_child_first() {
        let parent = Transform::translation(10.0, 0.0, 0.0);
        let child = Transform::scale(2.0, 2.0, 2.0);
        let world = parent.then_apply(&child);
        assert_eq!(world.apply_point([1.0, 0.0, 0.0]), Some([12.0, 0.0, 0.0]));
    }

    #[test]
    fn homogeneous_w_is_divided_out() {
        let mut t = Transform::IDENTITY;
        t.0[15] = 0.5;
        assert_eq!(t.apply_point([1.0, 2.0, 3.0]), Some([2.0, 4.0, 6.0]));
    }

    #[test]
    fn zero_w_yields_no_point() {
        let mut t = Transform::IDENTITY;
        t.0[15] = 0.0;
        assert_eq!(t.apply_point([1.0, 2.0, 3.0]), None);
    }

    #[test]
    fn mirror_is_detected_by_the_determinant() {
        assert!(!Transform::IDENTITY.is_mirroring());
        assert!(Transform::scale(-1.0, 1.0, 1.0).is_mirroring());
        assert!(!Transform::scale(-1.0, -1.0, 1.0).is_mirroring());
    }

    #[test]
    fn normal_follows_non_uniform_scale_by_inverse_transpose() {
        let t = Transform::scale(4.0, 1.0, 1.0);
        let n = t.apply_normal([1.0, 1.0, 0.0]);
        let expected = [
            0.25 / (0.0625f64 + 1.0).sqrt(),
            1.0 / (0.0625f64 + 1.0).sqrt(),
            0.0,
        ];
        assert!(close(n, expected), "{n:?}");
    }

    #[test]
    fn normal_through_a_mirror_stays_geometrically_correct() {
        let t = Transform::scale(-1.0, 1.0, 1.0);
        assert!(close(t.apply_normal([1.0, 0.0, 0.0]), [-1.0, 0.0, 0.0]));
        assert!(close(t.apply_normal([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn face_count_includes_nested_nodes() {
        let leaf = Node {
            faces: vec![SourceFace::default(); 2],
            ..Node::default()
        };
        let root = Node {
            faces: vec![SourceFace::default()],
            children: vec![leaf.clone(), leaf],
            ..Node::default()
        };
        assert_eq!(root.face_count(), 5);
    }
}
