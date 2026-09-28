use skp_core::geometry::Vec3;
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DEdge {
    pub x: u32,
    pub y: u32,
}

impl DEdge {
    pub fn new(a: u32, b: u32) -> DEdge {
        DEdge {
            x: a.min(b),
            y: a.max(b),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lattice {
    pub o: Vec3,
    pub q: Vec3,
    pub n: Vec3,
    pub scale: [f64; 2],
    pub inv_scale: [f64; 2],
}

impl Lattice {
    fn t(&self) -> Vec3 {
        self.n.cross(self.q)
    }

    fn corner(&self, base: Vec3, i: usize) -> Vec3 {
        base + self.q * ((i & 1) as f64 * self.scale[0])
            + self.t() * (((i & 2) >> 1) as f64 * self.scale[1])
    }
}

pub fn normalized(v: Vec3) -> Vec3 {
    let z = v.dot(v);
    if z > 0.0 {
        v / z.sqrt()
    } else {
        v
    }
}

fn single(value: f32) -> f64 {
    value as f64
}

pub fn fast_acos(x: f64) -> f64 {
    let negate = if x < 0.0 { 1.0 } else { 0.0 };
    let x = x.abs();
    let mut ret = single(-0.0187293);
    ret *= x;
    ret += single(0.074261);
    ret *= x;
    ret -= single(0.2121144);
    ret *= x;
    ret += single(1.5707288);
    ret *= (1.0 - x).sqrt();
    ret -= 2.0 * negate * ret;
    negate * PI + ret
}

pub fn modulo(a: i32, b: i32) -> i32 {
    let r = a % b;
    if r < 0 {
        r + b
    } else {
        r
    }
}

pub fn rotate90_by(q: Vec3, n: Vec3, amount: i32) -> Vec3 {
    let turned = if amount & 1 != 0 { n.cross(q) } else { q };
    turned * if amount < 2 { 1.0 } else { -1.0 }
}

pub fn rshift90(shift: [i32; 2], amount: i32) -> [i32; 2] {
    let mut s = shift;
    if amount & 1 != 0 {
        s = [-s[1], s[0]];
    }
    if amount >= 2 {
        s = [-s[0], -s[1]];
    }
    s
}

fn frames(q0: Vec3, n0: Vec3, q1: Vec3, n1: Vec3) -> ([Vec3; 2], [Vec3; 2]) {
    ([q0, n0.cross(q0)], [q1, n1.cross(q1)])
}

pub fn compat_orientation_extrinsic_index_4(q0: Vec3, n0: Vec3, q1: Vec3, n1: Vec3) -> (i32, i32) {
    let (a, b) = frames(q0, n0, q1, n1);
    let mut best_score = f64::NEG_INFINITY;
    let (mut best_a, mut best_b) = (0, 0);
    for (i, ai) in a.iter().enumerate() {
        for (j, bj) in b.iter().enumerate() {
            let score = ai.dot(*bj).abs();
            if score > best_score {
                best_a = i;
                best_b = j;
                best_score = score;
            }
        }
    }
    let flip = if a[best_a].dot(b[best_b]) < 0.0 { 2 } else { 0 };
    (best_a as i32, best_b as i32 + flip)
}

pub fn compat_orientation_extrinsic_4(q0: Vec3, n0: Vec3, q1: Vec3, n1: Vec3) -> (Vec3, Vec3) {
    let (a, b) = frames(q0, n0, q1, n1);
    let mut best_score = f64::NEG_INFINITY;
    let (mut best_a, mut best_b) = (0, 0);
    for (i, ai) in a.iter().enumerate() {
        for (j, bj) in b.iter().enumerate() {
            let score = ai.dot(*bj).abs();
            if score > best_score + 1e-6 {
                best_a = i;
                best_b = j;
                best_score = score;
            }
        }
    }
    let dp = a[best_a].dot(b[best_b]);
    (a[best_a], b[best_b] * 1.0f64.copysign(dp))
}

pub fn middle_point(p0: Vec3, n0: Vec3, p1: Vec3, n1: Vec3) -> Vec3 {
    let n0p0 = n0.dot(p0);
    let n0p1 = n0.dot(p1);
    let n1p0 = n1.dot(p0);
    let n1p1 = n1.dot(p1);
    let n0n1 = n0.dot(n1);
    let denom = 1.0 / (1.0 - n0n1 * n0n1 + single(1e-4));
    let lambda_0 = 2.0 * (n0p1 - n0p0 - n0n1 * (n1p0 - n1p1)) * denom;
    let lambda_1 = 2.0 * (n1p0 - n1p1 - n0n1 * (n0p1 - n0p0)) * denom;
    (p0 + p1) * 0.5 - (n0 * lambda_0 + n1 * lambda_1) * 0.25
}

fn lattice_steps(l: &Lattice, p: Vec3) -> (f64, f64) {
    let d = p - l.o;
    (l.q.dot(d) * l.inv_scale[0], l.t().dot(d) * l.inv_scale[1])
}

fn lattice_point(l: &Lattice, i: f64, j: f64) -> Vec3 {
    l.o + l.q * (i * l.scale[0]) + l.t() * (j * l.scale[1])
}

pub fn position_floor_4(l: &Lattice, p: Vec3) -> Vec3 {
    let (i, j) = lattice_steps(l, p);
    lattice_point(l, i.floor(), j.floor())
}

pub fn position_round_4(l: &Lattice, p: Vec3) -> Vec3 {
    let (i, j) = lattice_steps(l, p);
    lattice_point(l, i.round(), j.round())
}

pub fn position_floor_index_4(l: &Lattice, p: Vec3) -> [i32; 2] {
    let (i, j) = lattice_steps(l, p);
    [i.floor() as i32, j.floor() as i32]
}

fn squared_distance(a: Vec3, b: Vec3) -> f64 {
    let d = a - b;
    d.dot(d)
}

fn closest_corners(l0: &Lattice, base0: Vec3, l1: &Lattice, base1: Vec3) -> (usize, usize, f64) {
    let mut best_cost = f64::INFINITY;
    let (mut best_i, mut best_j) = (usize::MAX, usize::MAX);
    for i in 0..4 {
        let o0t = l0.corner(base0, i);
        for j in 0..4 {
            let cost = squared_distance(o0t, l1.corner(base1, j));
            if cost < best_cost {
                best_i = i;
                best_j = j;
                best_cost = cost;
            }
        }
    }
    (best_i, best_j, best_cost)
}

pub fn compat_position_extrinsic_4(p0: Vec3, l0: &Lattice, p1: Vec3, l1: &Lattice) -> (Vec3, Vec3) {
    let middle = middle_point(p0, l0.n, p1, l1.n);
    let o0p = position_floor_4(l0, middle);
    let o1p = position_floor_4(l1, middle);
    let (i, j, _) = closest_corners(l0, o0p, l1, o1p);
    (l0.corner(o0p, i), l1.corner(o1p, j))
}

pub fn compat_position_extrinsic_index_4(
    p0: Vec3,
    l0: &Lattice,
    p1: Vec3,
    l1: &Lattice,
) -> ([i32; 2], [i32; 2], f64) {
    let middle = middle_point(p0, l0.n, p1, l1.n);
    let o0p = position_floor_index_4(l0, middle);
    let o1p = position_floor_index_4(l1, middle);
    let mut best_cost = f64::INFINITY;
    let (mut best_i, mut best_j) = (0i32, 0i32);
    for i in 0..4i32 {
        let o0t = lattice_point(
            l0,
            ((i & 1) + o0p[0]) as f64,
            (((i & 2) >> 1) + o0p[1]) as f64,
        );
        for j in 0..4i32 {
            let o1t = lattice_point(
                l1,
                ((j & 1) + o1p[0]) as f64,
                (((j & 2) >> 1) + o1p[1]) as f64,
            );
            let cost = squared_distance(o0t, o1t);
            if cost < best_cost {
                best_i = i;
                best_j = j;
                best_cost = cost;
            }
        }
    }
    (
        [(best_i & 1) + o0p[0], ((best_i & 2) >> 1) + o0p[1]],
        [(best_j & 1) + o1p[0], ((best_j & 2) >> 1) + o1p[1]],
        best_cost,
    )
}

pub fn coordinate_system(a: Vec3) -> (Vec3, Vec3) {
    let c = if a.x.abs() > a.y.abs() {
        let inv_len = 1.0 / (a.x * a.x + a.z * a.z).sqrt();
        Vec3::new(a.z * inv_len, 0.0, -a.x * inv_len)
    } else {
        let inv_len = 1.0 / (a.y * a.y + a.z * a.z).sqrt();
        Vec3::new(0.0, a.z * inv_len, -a.y * inv_len)
    };
    (c.cross(a), c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x() -> Vec3 {
        Vec3::new(1.0, 0.0, 0.0)
    }

    fn y() -> Vec3 {
        Vec3::new(0.0, 1.0, 0.0)
    }

    fn z() -> Vec3 {
        Vec3::new(0.0, 0.0, 1.0)
    }

    fn unit_lattice(o: Vec3) -> Lattice {
        Lattice {
            o,
            q: x(),
            n: z(),
            scale: [1.0, 1.0],
            inv_scale: [1.0, 1.0],
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        squared_distance(a, b) < 1e-24
    }

    #[test]
    fn an_edge_is_stored_smallest_vertex_first() {
        assert_eq!(DEdge::new(7, 3), DEdge { x: 3, y: 7 });
        assert!(DEdge::new(1, 9) < DEdge::new(3, 2));
        assert!(DEdge::new(2, 3) < DEdge::new(4, 2));
    }

    #[test]
    fn fast_acos_is_exact_at_the_ends_and_close_between() {
        assert_eq!(fast_acos(1.0), 0.0);
        assert_eq!(fast_acos(-1.0), PI);
        for k in -100..=100 {
            let v = k as f64 / 100.0;
            assert!((fast_acos(v) - v.acos()).abs() < 1e-4, "{v}");
        }
    }

    #[test]
    fn normalized_divides_by_the_norm_and_leaves_zero_alone() {
        assert_eq!(
            normalized(Vec3::new(0.0, 3.0, 4.0)),
            Vec3::new(0.0, 0.6, 0.8)
        );
        assert_eq!(normalized(Vec3::default()), Vec3::default());
    }

    #[test]
    fn modulo_is_never_negative() {
        assert_eq!(modulo(-1, 4), 3);
        assert_eq!(modulo(-8, 4), 0);
        assert_eq!(modulo(9, 4), 1);
    }

    #[test]
    fn rotate90_by_turns_a_quarter_per_step_about_the_normal() {
        assert_eq!(rotate90_by(x(), z(), 0), x());
        assert_eq!(rotate90_by(x(), z(), 1), y());
        assert_eq!(rotate90_by(x(), z(), 2), x() * -1.0);
        assert_eq!(rotate90_by(x(), z(), 3), y() * -1.0);
    }

    #[test]
    fn rshift90_turns_an_integer_offset_a_quarter_per_step() {
        assert_eq!(rshift90([1, 2], 0), [1, 2]);
        assert_eq!(rshift90([1, 2], 1), [-2, 1]);
        assert_eq!(rshift90([1, 2], 2), [-1, -2]);
        assert_eq!(rshift90([1, 2], 3), [2, -1]);
    }

    #[test]
    fn orientation_index_names_the_rotation_that_matches_the_two_crosses() {
        let (a, b) = compat_orientation_extrinsic_index_4(x(), z(), y(), z());
        assert_eq!((a, b), (0, 3));
        assert_eq!(rotate90_by(x(), z(), a), rotate90_by(y(), z(), b));
    }

    #[test]
    fn orientation_pair_returns_the_closest_matching_directions() {
        let (a, b) = compat_orientation_extrinsic_4(x(), z(), y(), z());
        assert_eq!(a, x());
        assert_eq!(b, x());
    }

    #[test]
    fn middle_point_of_two_points_on_one_plane_is_their_midpoint() {
        let m = middle_point(Vec3::default(), z(), Vec3::new(2.0, 0.0, 0.0), z());
        assert!(close(m, Vec3::new(1.0, 0.0, 0.0)));
    }

    #[test]
    fn middle_point_of_two_stacked_parallel_planes_lies_between_them() {
        let m = middle_point(Vec3::default(), z(), Vec3::new(0.0, 0.0, 2.0), z());
        assert!(close(m, Vec3::new(0.0, 0.0, 1.0)));
    }

    #[test]
    fn lattice_floor_and_round_follow_the_c_library() {
        let l = unit_lattice(Vec3::default());
        let p = Vec3::new(2.5, -0.5, 0.0);
        assert_eq!(position_floor_4(&l, p), Vec3::new(2.0, -1.0, 0.0));
        assert_eq!(position_floor_index_4(&l, p), [2, -1]);
        assert_eq!(position_round_4(&l, p), Vec3::new(3.0, -1.0, 0.0));
    }

    #[test]
    fn compatible_positions_are_the_closest_lattice_corners() {
        let l0 = unit_lattice(Vec3::default());
        let l1 = unit_lattice(Vec3::new(0.5, 0.0, 0.0));
        let p0 = Vec3::new(0.2, 0.2, 0.0);
        let p1 = Vec3::new(0.9, 0.3, 0.0);
        let (a, b) = compat_position_extrinsic_4(p0, &l0, p1, &l1);
        assert_eq!(a, Vec3::default());
        assert_eq!(b, Vec3::new(0.5, 0.0, 0.0));
        let (i, j, error) = compat_position_extrinsic_index_4(p0, &l0, p1, &l1);
        assert_eq!((i, j), ([0, 0], [0, 0]));
        assert_eq!(error, 0.25);
    }

    #[test]
    fn coordinate_system_completes_an_orthonormal_frame() {
        let (b, c) = coordinate_system(z());
        assert_eq!(b, x());
        assert_eq!(c, y());
        let a = Vec3::new(3.0, -1.0, 2.0).normalised().unwrap();
        let (b, c) = coordinate_system(a);
        assert!(a.dot(b).abs() < 1e-12 && a.dot(c).abs() < 1e-12 && b.dot(c).abs() < 1e-12);
        assert!((b.length() - 1.0).abs() < 1e-12 && (c.length() - 1.0).abs() < 1e-12);
    }
}
