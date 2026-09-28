use crate::error::RetopoError;
use skp_core::correspondence::{Correspondence, CorrespondenceBuilder};
use skp_core::geometry::Vec3;

pub fn closest_point_on_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denom = 1.0 / (va + vb + vc);
    a + ab * (vb * denom) + ac * (vc * denom)
}

fn centroid([a, b, c]: [Vec3; 3]) -> Vec3 {
    (a + b + c) / 3.0
}

pub struct TriangleGrid<'a> {
    triangles: &'a [[Vec3; 3]],
    origin: Vec3,
    cell: f64,
    dims: [usize; 3],
    cells: Vec<Vec<u32>>,
}

impl<'a> TriangleGrid<'a> {
    pub fn new(triangles: &'a [[Vec3; 3]]) -> TriangleGrid<'a> {
        let mut min = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut max = Vec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for t in triangles {
            for p in t {
                min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
            }
        }
        let extent = (max.x - min.x).max(max.y - min.y).max(max.z - min.z);
        let per_axis = (triangles.len() as f64).cbrt().ceil().max(1.0);
        let cell = if extent > 0.0 { extent / per_axis } else { 1.0 };
        let axis = |lo: f64, hi: f64| (((hi - lo) / cell).floor() as usize + 1).max(1);
        let dims = [axis(min.x, max.x), axis(min.y, max.y), axis(min.z, max.z)];
        let mut grid = TriangleGrid {
            triangles,
            origin: min,
            cell,
            dims,
            cells: vec![Vec::new(); dims[0] * dims[1] * dims[2]],
        };
        for (i, t) in triangles.iter().enumerate() {
            let lo = grid.cell_of(Vec3::new(
                t[0].x.min(t[1].x).min(t[2].x),
                t[0].y.min(t[1].y).min(t[2].y),
                t[0].z.min(t[1].z).min(t[2].z),
            ));
            let hi = grid.cell_of(Vec3::new(
                t[0].x.max(t[1].x).max(t[2].x),
                t[0].y.max(t[1].y).max(t[2].y),
                t[0].z.max(t[1].z).max(t[2].z),
            ));
            for z in lo[2]..=hi[2] {
                for y in lo[1]..=hi[1] {
                    for x in lo[0]..=hi[0] {
                        let index = grid.index([x, y, z]);
                        grid.cells[index].push(i as u32);
                    }
                }
            }
        }
        grid
    }

    fn cell_of(&self, p: Vec3) -> [usize; 3] {
        let clamp = |v: f64, lo: f64, n: usize| {
            let c = ((v - lo) / self.cell).floor();
            if c <= 0.0 {
                0
            } else {
                (c as usize).min(n - 1)
            }
        };
        [
            clamp(p.x, self.origin.x, self.dims[0]),
            clamp(p.y, self.origin.y, self.dims[1]),
            clamp(p.z, self.origin.z, self.dims[2]),
        ]
    }

    fn index(&self, [x, y, z]: [usize; 3]) -> usize {
        (z * self.dims[1] + y) * self.dims[0] + x
    }

    pub fn nearest(&self, p: Vec3) -> Option<u32> {
        let home = self.cell_of(p);
        let max_ring = self.dims.iter().copied().max().unwrap_or(1);
        let mut best: Option<(f64, u32)> = None;
        for ring in 0..=max_ring {
            let lo = home.map(|c| c.saturating_sub(ring));
            let hi = [0, 1, 2].map(|k| (home[k] + ring).min(self.dims[k] - 1));
            for z in lo[2]..=hi[2] {
                for y in lo[1]..=hi[1] {
                    for x in lo[0]..=hi[0] {
                        let on_ring = [x, y, z]
                            .iter()
                            .zip(&home)
                            .any(|(&c, &h)| c.abs_diff(h) == ring);
                        if !on_ring {
                            continue;
                        }
                        for &t in &self.cells[self.index([x, y, z])] {
                            let q = closest_point_on_triangle(p, self.triangles[t as usize]);
                            let d = (q - p).dot(q - p);
                            let better = match best {
                                None => true,
                                Some((bd, bt)) => d < bd || (d == bd && t < bt),
                            };
                            if better {
                                best = Some((d, t));
                            }
                        }
                    }
                }
            }
            if let Some((d, _)) = best {
                let reach = ring as f64 * self.cell;
                if d <= reach * reach {
                    break;
                }
            }
        }
        best.map(|(_, t)| t)
    }
}

pub fn geometric_correspondence(
    low: &[[Vec3; 3]],
    high: &[[Vec3; 3]],
) -> Result<Correspondence, RetopoError> {
    let mut builder = CorrespondenceBuilder::new(low.len());
    let mut covered = vec![false; low.len()];
    let low_grid = TriangleGrid::new(low);
    for (h, t) in high.iter().enumerate() {
        if let Some(l) = low_grid.nearest(centroid(*t)) {
            builder.push(l, h as u32);
            covered[l as usize] = true;
        }
    }
    if covered.iter().any(|&c| !c) {
        let high_grid = TriangleGrid::new(high);
        for (l, t) in low.iter().enumerate() {
            if covered[l] {
                continue;
            }
            if let Some(h) = high_grid.nearest(centroid(*t)) {
                builder.push(l as u32, h);
            }
        }
    }
    let map = builder.build();
    map.validate()?;
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(x: f64, y: f64, z: f64) -> Vec3 {
        Vec3::new(x, y, z)
    }

    fn strip(count: usize, width: f64) -> Vec<[Vec3; 3]> {
        (0..count)
            .flat_map(|i| {
                let (x0, x1) = (i as f64 * width, (i + 1) as f64 * width);
                [
                    [v(x0, 0.0, 0.0), v(x1, 0.0, 0.0), v(x1, 1.0, 0.0)],
                    [v(x0, 0.0, 0.0), v(x1, 1.0, 0.0), v(x0, 1.0, 0.0)],
                ]
            })
            .collect()
    }

    #[test]
    fn the_closest_point_is_found_in_every_region_of_the_triangle() {
        let t = [v(0.0, 0.0, 0.0), v(2.0, 0.0, 0.0), v(0.0, 2.0, 0.0)];
        assert_eq!(
            closest_point_on_triangle(v(0.5, 0.5, 3.0), t),
            v(0.5, 0.5, 0.0)
        );
        assert_eq!(
            closest_point_on_triangle(v(-1.0, -1.0, 0.0), t),
            v(0.0, 0.0, 0.0)
        );
        assert_eq!(
            closest_point_on_triangle(v(1.0, -1.0, 0.0), t),
            v(1.0, 0.0, 0.0)
        );
        assert_eq!(
            closest_point_on_triangle(v(2.0, 2.0, 0.0), t),
            v(1.0, 1.0, 0.0)
        );
        assert_eq!(
            closest_point_on_triangle(v(5.0, -1.0, 0.0), t),
            v(2.0, 0.0, 0.0)
        );
    }

    #[test]
    fn the_grid_finds_the_same_triangle_as_a_brute_force_search() {
        let tris = strip(40, 0.25);
        let grid = TriangleGrid::new(&tris);
        for k in 0..200 {
            let p = v(
                (k as f64 * 0.37) % 11.0 - 0.5,
                (k as f64 * 0.61) % 1.6 - 0.3,
                (k as f64 * 0.13) % 0.4,
            );
            let brute = (0..tris.len() as u32)
                .min_by(|&a, &b| {
                    let da = (closest_point_on_triangle(p, tris[a as usize]) - p).length();
                    let db = (closest_point_on_triangle(p, tris[b as usize]) - p).length();
                    da.partial_cmp(&db).unwrap().then(a.cmp(&b))
                })
                .unwrap();
            let got = grid.nearest(p).unwrap();
            let dist = |t: u32| (closest_point_on_triangle(p, tris[t as usize]) - p).length();
            assert_eq!(dist(got), dist(brute), "point {p:?}");
        }
    }

    #[test]
    fn every_low_triangle_maps_and_every_high_triangle_is_used() {
        let low = strip(2, 2.0);
        let high = strip(16, 0.25);
        let map = geometric_correspondence(&low, &high).unwrap();
        assert_eq!(map.low_triangle_count(), 4);
        assert_eq!(map.pair_count(), high.len());
        for l in 0..4 {
            assert!(!map.high_for(l).is_empty());
        }
    }

    #[test]
    fn a_low_triangle_no_high_centroid_reaches_still_gets_its_nearest() {
        let low = strip(4, 1.0);
        let high = strip(1, 1.0);
        let map = geometric_correspondence(&low, &high).unwrap();
        for l in 0..8 {
            assert!(!map.high_for(l).is_empty(), "low {l}");
        }
    }
}
