use super::dedge::INVALID;
use super::field_math::{
    compat_orientation_extrinsic_4, compat_orientation_extrinsic_index_4, modulo,
};
use super::hierarchy::{Hierarchy, Level, RCP_OVERFLOW};
use std::collections::BTreeMap;

const LEVEL_ITERATIONS: usize = 6;

fn smooth_level(h: &mut Hierarchy, level: usize) {
    let Level {
        phases, adj, n, q, ..
    } = &mut h.levels[level];
    for _ in 0..LEVEL_ITERATIONS {
        for phase in phases.iter() {
            for &i in phase {
                let i = i as usize;
                let n_i = n[i];
                let mut weight_sum = 0.0;
                let mut sum = q[i];
                for link in &adj[i] {
                    let weight = link.weight;
                    if weight == 0.0 {
                        continue;
                    }
                    let j = link.id as usize;
                    let (a, b) = compat_orientation_extrinsic_4(sum, n_i, q[j], n[j]);
                    sum = a * weight_sum + b * weight;
                    sum = sum - n_i * n_i.dot(sum);
                    weight_sum += weight;
                    let norm = sum.length();
                    if norm > RCP_OVERFLOW {
                        sum = sum / norm;
                    }
                }
                if weight_sum > 0.0 {
                    q[i] = sum;
                }
            }
        }
    }
}

fn push_to_finer(h: &mut Hierarchy, level: usize) {
    let (finer, coarser) = h.levels.split_at_mut(level);
    let (dest, src) = (&mut finer[level - 1], &coarser[0]);
    for (i, upper) in h.to_upper[level - 1].iter().enumerate() {
        for &d in upper {
            if d == INVALID {
                continue;
            }
            let (q, n) = (src.q[i], dest.n[d as usize]);
            dest.q[d as usize] = q - n * n.dot(q);
        }
    }
}

fn pull_to_coarser(h: &mut Hierarchy, level: usize) {
    let (finer, coarser) = h.levels.split_at_mut(level + 1);
    let (src, dest) = (&finer[level], &mut coarser[0]);
    for (i, upper) in h.to_upper[level].iter().enumerate() {
        let (q0, n0) = (src.q[upper[0] as usize], src.n[upper[0] as usize]);
        let mut q = if upper[1] != INVALID {
            let k = upper[1] as usize;
            let (a, b) = compat_orientation_extrinsic_4(q0, n0, src.q[k], src.n[k]);
            a + b
        } else {
            q0
        };
        let n = dest.n[i];
        q = q - n * n.dot(q);
        if q.dot(q) > RCP_OVERFLOW {
            q = q / q.length();
        }
        dest.q[i] = q;
    }
}

pub fn optimize_orientations(h: &mut Hierarchy) {
    for level in (0..h.levels.len()).rev() {
        smooth_level(h, level);
        if level > 0 {
            push_to_finer(h, level);
        }
    }
    for level in 0..h.levels.len() - 1 {
        pull_to_coarser(h, level);
    }
}

pub fn orientation_singularities(h: &mut Hierarchy) -> BTreeMap<u32, i32> {
    let mut singularities = BTreeMap::new();
    let l = &mut h.levels[0];
    for (f, face) in h.faces.iter().enumerate() {
        let mut index = 0;
        for k in 0..3 {
            let (i, j) = (face[k] as usize, face[(k + 1) % 3] as usize);
            let (a, b) = compat_orientation_extrinsic_index_4(l.q[i], l.n[i], l.q[j], l.n[j]);
            index += b - a;
        }
        let index_mod = modulo(index, 4);
        if index_mod == 1 || index_mod == 3 {
            if !(0..4).contains(&index) {
                let v = face[0] as usize;
                l.q[v] = -l.q[v];
            }
            singularities.insert(f as u32, index_mod);
        }
    }
    singularities
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::route_b::pcg32::Pcg32;
    use crate::route_b::Parametrizer;
    use skp_core::geometry::Vec3;

    pub(crate) fn cube() -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let v = (0..8)
            .map(|i| Vec3::new((i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64))
            .collect();
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        let f = quads
            .iter()
            .flat_map(|q: &[u32; 4]| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect();
        (v, f)
    }

    pub(crate) fn flat(side: u32) -> (Vec<Vec3>, Vec<[u32; 3]>) {
        let id = |x: u32, y: u32| y * (side + 1) + x;
        let v = (0..=side)
            .flat_map(|y| (0..=side).map(move |x| Vec3::new(x as f64, y as f64, 0.0)))
            .collect();
        let mut f = Vec::new();
        for y in 0..side {
            for x in 0..side {
                f.push([id(x, y), id(x + 1, y), id(x + 1, y + 1)]);
                f.push([id(x, y), id(x + 1, y + 1), id(x, y + 1)]);
            }
        }
        (v, f)
    }

    pub(crate) fn initialised(mesh: (Vec<Vec3>, Vec<[u32; 3]>), faces: usize) -> Parametrizer {
        let mut p = Parametrizer::load(&mesh.0, &mesh.1);
        p.initialize(faces, &mut Pcg32::seeded(3, 1));
        p
    }

    #[test]
    fn the_field_stays_unit_and_tangent() {
        let mut p = initialised(cube(), 300);
        optimize_orientations(&mut p.hierarchy);
        for level in &p.hierarchy.levels {
            for (q, n) in level.q.iter().zip(&level.n) {
                assert!((q.length() - 1.0).abs() < 1e-9);
                assert!(q.dot(*n).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn a_flat_patch_gets_a_field_with_no_singularity() {
        let mut p = initialised(flat(4), 200);
        optimize_orientations(&mut p.hierarchy);
        assert!(orientation_singularities(&mut p.hierarchy).is_empty());
    }

    #[test]
    fn a_cube_carries_a_net_index_of_eight_quarter_turns() {
        let mut p = initialised(cube(), 300);
        optimize_orientations(&mut p.hierarchy);
        let s = orientation_singularities(&mut p.hierarchy);
        let plus = s.values().filter(|&&v| v == 1).count() as i64;
        let minus = s.values().filter(|&&v| v == 3).count() as i64;
        assert_eq!(plus - minus, 8);
    }
}
