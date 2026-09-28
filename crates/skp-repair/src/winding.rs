use crate::geometry::{area_vector, Vec3};
use std::f64::consts::PI;

const LEAF_SIZE: usize = 8;
const FAR_FIELD_RATIO: f64 = 2.0;

struct Node {
    center: Vec3,
    radius: f64,
    area_normal: Vec3,
    children: Option<(usize, usize)>,
    start: usize,
    end: usize,
}

pub struct WindingField {
    triangles: Vec<[Vec3; 3]>,
    nodes: Vec<Node>,
}

fn centroid(t: &[Vec3; 3]) -> Vec3 {
    (t[0] + t[1] + t[2]) * (1.0 / 3.0)
}

fn axis(v: Vec3, k: usize) -> f64 {
    match k {
        0 => v.x,
        1 => v.y,
        _ => v.z,
    }
}

pub fn solid_angle(t: &[Vec3; 3], p: Vec3) -> f64 {
    let a = t[0] - p;
    let b = t[1] - p;
    let c = t[2] - p;
    let (la, lb, lc) = (a.length(), b.length(), c.length());
    let numerator = a.dot(b.cross(c));
    let denominator = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
    2.0 * numerator.atan2(denominator)
}

impl WindingField {
    pub fn new(triangles: Vec<[Vec3; 3]>) -> WindingField {
        let mut field = WindingField {
            triangles,
            nodes: Vec::new(),
        };
        if !field.triangles.is_empty() {
            field.build(0, field.triangles.len());
        }
        field
    }

    fn build(&mut self, start: usize, end: usize) -> usize {
        let slice = &self.triangles[start..end];
        let mut area_normal = Vec3::default();
        let mut weighted = Vec3::default();
        let mut total_area = 0.0;
        let mut lo = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut hi = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in slice {
            let n = area_vector(t[0], t[1], t[2]) * 0.5;
            let area = n.length();
            area_normal = area_normal + n;
            weighted = weighted + centroid(t) * area;
            total_area += area;
            for p in t {
                lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
                hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
            }
        }
        let center = if total_area > 0.0 {
            weighted * (1.0 / total_area)
        } else {
            (lo + hi) * 0.5
        };
        let radius = slice
            .iter()
            .flat_map(|t| t.iter())
            .map(|&p| (p - center).length())
            .fold(0.0, f64::max);
        let index = self.nodes.len();
        self.nodes.push(Node {
            center,
            radius,
            area_normal,
            children: None,
            start,
            end,
        });
        if end - start > LEAF_SIZE {
            let extent = hi - lo;
            let k = if extent.x >= extent.y && extent.x >= extent.z {
                0
            } else if extent.y >= extent.z {
                1
            } else {
                2
            };
            let mid = start + (end - start) / 2;
            self.triangles[start..end].select_nth_unstable_by(mid - start, |a, b| {
                axis(centroid(a), k).total_cmp(&axis(centroid(b), k))
            });
            let left = self.build(start, mid);
            let right = self.build(mid, end);
            self.nodes[index].children = Some((left, right));
        }
        index
    }

    pub fn at(&self, p: Vec3) -> f64 {
        if self.nodes.is_empty() {
            return 0.0;
        }
        let mut total = 0.0;
        let mut stack = vec![0usize];
        while let Some(i) = stack.pop() {
            let node = &self.nodes[i];
            let offset = node.center - p;
            let distance = offset.length();
            if distance > FAR_FIELD_RATIO * node.radius {
                total += node.area_normal.dot(offset) / (distance * distance * distance);
                continue;
            }
            match node.children {
                Some((left, right)) => {
                    stack.push(left);
                    stack.push(right);
                }
                None => {
                    total += self.triangles[node.start..node.end]
                        .iter()
                        .map(|t| solid_angle(t, p))
                        .sum::<f64>();
                }
            }
        }
        total / (4.0 * PI)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::cube;

    fn cube_field(size: f64, subdivisions: usize) -> WindingField {
        let (points, tris) = cube(size);
        let mut triangles = Vec::new();
        for tri in tris {
            let [a, b, c] = tri.map(|i| Vec3::of(points[i as usize]));
            let n = subdivisions as f64;
            for i in 0..subdivisions {
                for j in 0..subdivisions - i {
                    let at = |u: f64, v: f64| a + (b - a) * (u / n) + (c - a) * (v / n);
                    let (u, v) = (i as f64, j as f64);
                    triangles.push([at(u, v), at(u + 1.0, v), at(u, v + 1.0)]);
                    if j + 1 < subdivisions - i {
                        triangles.push([at(u + 1.0, v), at(u + 1.0, v + 1.0), at(u, v + 1.0)]);
                    }
                }
            }
        }
        WindingField::new(triangles)
    }

    #[test]
    fn a_point_inside_a_closed_outward_shell_winds_once() {
        let field = cube_field(2.0, 1);
        assert!((field.at(Vec3::new(1.0, 1.0, 1.0)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_point_outside_winds_zero() {
        let field = cube_field(2.0, 1);
        assert!(field.at(Vec3::new(5.0, 1.0, 1.0)).abs() < 1e-9);
    }

    #[test]
    fn the_far_field_approximation_stays_close_to_exact() {
        let field = cube_field(2.0, 12);
        assert!(field.nodes.len() > 1);
        for p in [
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(0.1, 1.9, 0.5),
            Vec3::new(2.1, 1.0, 1.0),
            Vec3::new(-3.0, 4.0, 1.0),
        ] {
            let exact: f64 = field
                .triangles
                .iter()
                .map(|t| solid_angle(t, p))
                .sum::<f64>()
                / (4.0 * PI);
            assert!((field.at(p) - exact).abs() < 0.05, "{p:?}");
        }
    }

    #[test]
    fn an_open_sheet_gives_a_fraction_that_is_not_inside() {
        let (points, tris) = cube(2.0);
        let top: Vec<[Vec3; 3]> = tris[2..4]
            .iter()
            .map(|tri| tri.map(|i| Vec3::of(points[i as usize])))
            .collect();
        let field = WindingField::new(top);
        let w = field.at(Vec3::new(1.0, 1.0, 1.0));
        assert!(w.abs() > 0.05 && w.abs() < 0.5);
    }

    #[test]
    fn nothing_winds_nothing() {
        assert_eq!(WindingField::new(Vec::new()).at(Vec3::default()), 0.0);
    }
}
