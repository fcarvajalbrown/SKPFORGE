use super::dedge::INVALID;
use super::field_math::{
    compat_position_extrinsic_4, compat_position_extrinsic_index_4, normalized, position_round_4,
    rotate90_by, rshift90, Lattice,
};
use super::hierarchy::{Hierarchy, Level, RCP_OVERFLOW};
use skp_core::geometry::Vec3;
use std::collections::BTreeMap;

const LEVEL_ITERATIONS: usize = 6;

fn lattice(o: Vec3, q: Vec3, n: Vec3, scale: f64) -> Lattice {
    Lattice {
        o,
        q,
        n,
        scale: [scale, scale],
        inv_scale: [1.0 / scale, 1.0 / scale],
    }
}

fn smooth_level(h: &mut Hierarchy, level: usize) {
    let scale = h.scale;
    let Level {
        phases,
        adj,
        n,
        q,
        v,
        o,
        ..
    } = &mut h.levels[level];
    for _ in 0..LEVEL_ITERATIONS {
        for phase in phases.iter() {
            for &i in phase {
                let i = i as usize;
                let (n_i, v_i) = (n[i], v[i]);
                let q_i = normalized(q[i]);
                let mut sum = o[i];
                let mut weight_sum = 0.0;
                for link in &adj[i] {
                    let weight = link.weight;
                    if weight == 0.0 {
                        continue;
                    }
                    let j = link.id as usize;
                    let (a, b) = compat_position_extrinsic_4(
                        v_i,
                        &lattice(sum, q_i, n_i, scale),
                        v[j],
                        &lattice(o[j], normalized(q[j]), n[j], scale),
                    );
                    sum = a * weight_sum + b * weight;
                    weight_sum += weight;
                    if weight_sum > RCP_OVERFLOW {
                        sum = sum / weight_sum;
                    }
                    sum = sum - n_i * n_i.dot(sum - v_i);
                }
                if weight_sum > 0.0 {
                    o[i] = position_round_4(&lattice(sum, q_i, n_i, scale), v_i);
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
            let d = d as usize;
            let (n, v) = (dest.n[d], dest.v[d]);
            let o = src.o[i];
            dest.o[d] = o - n * n.dot(o - v);
        }
    }
}

pub fn optimize_positions(h: &mut Hierarchy) {
    for level in (0..h.levels.len()).rev() {
        smooth_level(h, level);
        if level > 0 {
            push_to_finer(h, level);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PositionSingularities {
    pub singular: BTreeMap<u32, [i32; 2]>,
    pub rank: Vec<[i32; 3]>,
    pub index: Vec<[i32; 6]>,
}

pub fn position_singularities(h: &Hierarchy) -> PositionSingularities {
    let l = &h.levels[0];
    let scale = h.scale;
    let mut out = PositionSingularities::default();
    for (f, face) in h.faces.iter().enumerate() {
        let ids = face.map(|i| i as usize);
        let mut q = ids.map(|i| normalized(l.q[i]));
        let n = ids.map(|i| l.n[i]);
        let o = ids.map(|i| l.o[i]);
        let v = ids.map(|i| l.v[i]);

        let mut best = [0i32; 3];
        let mut best_dp = f64::NEG_INFINITY;
        for i in 0..4 {
            let v0 = rotate90_by(q[0], n[0], i);
            for j in 0..4 {
                let v1 = rotate90_by(q[1], n[1], j);
                for k in 0..4 {
                    let v2 = rotate90_by(q[2], n[2], k);
                    let dp = v0.dot(v1).min(v1.dot(v2)).min(v2.dot(v0));
                    if dp > best_dp {
                        best_dp = dp;
                        best = [i, j, k];
                    }
                }
            }
        }
        out.rank.push(best);
        for k in 0..3 {
            q[k] = rotate90_by(q[k], n[k], best[k]);
        }

        let mut index = [0i32; 2];
        let mut diffs = [0i32; 6];
        for k in 0..3 {
            let kn = (k + 1) % 3;
            let (a, b, _) = compat_position_extrinsic_index_4(
                v[k],
                &lattice(o[k], q[k], n[k], scale),
                v[kn],
                &lattice(o[kn], q[kn], n[kn], scale),
            );
            let diff = [a[0] - b[0], a[1] - b[1]];
            index = [index[0] + diff[0], index[1] + diff[1]];
            diffs[k * 2] = diff[0];
            diffs[k * 2 + 1] = diff[1];
        }
        out.index.push(diffs);
        if index != [0, 0] {
            out.singular.insert(f as u32, rshift90(index, best[0]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_b::orient::optimize_orientations;
    use crate::route_b::orient::tests::{cube, flat, initialised};

    fn solved(p: &mut crate::route_b::Parametrizer) {
        optimize_orientations(&mut p.hierarchy);
        optimize_positions(&mut p.hierarchy);
    }

    #[test]
    fn every_position_stays_on_its_tangent_plane() {
        let mut p = initialised(cube(), 300);
        solved(&mut p);
        let l = &p.hierarchy.levels[0];
        for i in 0..l.v.len() {
            assert!(l.n[i].dot(l.o[i] - l.v[i]).abs() < 1e-9);
        }
    }

    #[test]
    fn a_flat_patch_gets_a_position_field_with_no_singularity() {
        let mut p = initialised(flat(4), 200);
        solved(&mut p);
        let s = position_singularities(&p.hierarchy);
        assert!(s.singular.is_empty(), "{:?}", s.singular);
        assert_eq!(s.rank.len(), p.hierarchy.faces.len());
    }

    #[test]
    fn on_a_flat_patch_neighbours_agree_on_the_lattice() {
        let mut p = initialised(flat(4), 200);
        solved(&mut p);
        let h = &p.hierarchy;
        let l = &h.levels[0];
        let mut worst: f64 = 0.0;
        for (i, links) in l.adj.iter().enumerate() {
            for link in links {
                let j = link.id as usize;
                let (_, _, error) = compat_position_extrinsic_index_4(
                    l.v[i],
                    &lattice(l.o[i], l.q[i], l.n[i], h.scale),
                    l.v[j],
                    &lattice(l.o[j], l.q[j], l.n[j], h.scale),
                );
                worst = worst.max(error);
            }
        }
        assert!(
            worst < 1e-6 * h.scale * h.scale,
            "worst squared gap {worst}"
        );
    }
}
