use skp_core::mesh::{Mesh, Point};
use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3 { x, y, z }
    }

    pub fn of(p: Point) -> Vec3 {
        Vec3::new(p.x.0, p.y.0, p.z.0)
    }

    pub fn dot(self, o: Vec3) -> f64 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        Vec3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    pub fn normalised(self) -> Option<Vec3> {
        let len = self.length();
        if len == 0.0 || !len.is_finite() {
            return None;
        }
        Some(self * (1.0 / len))
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f64) -> Vec3 {
        Vec3::new(self.x * k, self.y * k, self.z * k)
    }
}

pub fn area_vector(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    (b - a).cross(c - a)
}

pub fn height(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    let longest = (b - a).length().max((c - b).length()).max((a - c).length());
    if longest == 0.0 {
        return 0.0;
    }
    area_vector(a, b, c).length() / longest
}

pub fn triangle_positions(mesh: &Mesh, face: usize) -> [u32; 3] {
    let c = mesh.faces[face].corners();
    [c[0], c[1], c[2]].map(|corner| mesh.corners[corner as usize].position)
}

pub fn triangle_points(mesh: &Mesh, face: usize) -> [Vec3; 3] {
    triangle_positions(mesh, face).map(|p| Vec3::of(mesh.positions[p as usize]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_follows_the_right_hand_rule() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(x.cross(y), Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn area_vector_is_twice_the_area_along_the_normal() {
        let n = area_vector(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
            Vec3::new(0.0, 3.0, 0.0),
        );
        assert_eq!(n, Vec3::new(0.0, 0.0, 6.0));
    }

    #[test]
    fn height_is_measured_off_the_longest_edge() {
        let h = height(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(5.0, 0.5, 0.0),
        );
        assert!((h - 0.5).abs() < 1e-12);
    }

    #[test]
    fn a_zero_vector_has_no_direction() {
        assert_eq!(Vec3::default().normalised(), None);
        assert_eq!(
            Vec3::new(0.0, 3.0, 0.0).normalised(),
            Some(Vec3::new(0.0, 1.0, 0.0))
        );
    }
}
