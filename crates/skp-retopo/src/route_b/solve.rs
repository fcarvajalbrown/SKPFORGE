use super::dset::DisjointTree;
use super::field_math::{compat_orientation_extrinsic_index_4, rotate90_by};
use super::hierarchy::Hierarchy;
use super::integer::EdgeInfo;
use super::sparse::{Cholesky, SymmetricMatrix};
use crate::error::RetopoError;
use skp_core::geometry::Vec3;
use std::collections::BTreeMap;

pub const PULL: f64 = 1e-8;

pub fn solve_with_pull(
    rows: &[BTreeMap<u32, f64>],
    b: &[f64],
    current: &[f64],
) -> Result<Vec<f64>, RetopoError> {
    let n = rows.len();
    let diagonal: f64 = rows
        .iter()
        .enumerate()
        .map(|(i, row)| row.get(&(i as u32)).copied().unwrap_or(0.0))
        .sum();
    let eps = PULL * diagonal / n.max(1) as f64;
    let mut triplets = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        for (&j, &value) in row {
            triplets.push((i as u32, j, value));
        }
        triplets.push((i as u32, i as u32, eps));
    }
    let rhs: Vec<f64> = b
        .iter()
        .zip(current)
        .map(|(bi, xi)| bi + eps * xi)
        .collect();
    let a = SymmetricMatrix::from_triplets(n, &triplets);
    Ok(Cholesky::factor(&a)?.solve(&rhs))
}

fn add_to(row: &mut BTreeMap<u32, f64>, col: u32, value: f64) {
    *row.entry(col).or_insert(0.0) += value;
}

pub fn optimize_positions_fixed(h: &mut Hierarchy, info: &EdgeInfo) -> Result<(), RetopoError> {
    let scale = h.scale;
    let l = &mut h.levels[0];
    let vertex_count = l.v.len();
    let mut tree = DisjointTree::new(vertex_count);
    for (e, d) in info.edge_diff.iter().enumerate() {
        if d[0] == 0 && d[1] == 0 {
            let ev = info.edge_values[e];
            tree.merge(ev.x as usize, ev.y as usize);
        }
    }
    tree.build_compact_parent();
    let num = tree.compact_num();

    let mut positions = vec![Vec3::default(); num];
    let mut counts = vec![0usize; num];
    for i in 0..vertex_count {
        let p = tree.index(i);
        positions[p] = positions[p] + l.o[i];
        counts[p] += 1;
    }
    for (p, &c) in positions.iter_mut().zip(&counts) {
        if c > 0 {
            *p = *p / c as f64;
        }
    }
    let mut distance = vec![1e30; num];
    let mut index = vec![usize::MAX; num];
    for i in 0..vertex_count {
        let p = tree.index(i);
        let d = positions[p] - l.v[i];
        let dis = d.dot(d);
        if dis < distance[p] {
            distance[p] = dis;
            index[p] = i;
        }
    }

    let mut ideal: Vec<BTreeMap<usize, (u32, Vec3)>> = vec![BTreeMap::new(); num];
    for (e, ev) in info.edge_values.iter().enumerate() {
        let (v1, v2) = (ev.x as usize, ev.y as usize);
        let (p1, p2) = (tree.index(v1), tree.index(v2));
        let (q1, q2) = (index[p1], index[p2]);
        let (q_1, q_2, n_1, n_2) = (l.q[v1], l.q[v2], l.n[v1], l.n[v2]);
        let (q_1_y, q_2_y) = (n_1.cross(q_1), n_2.cross(q_2));
        let (a, b) = compat_orientation_extrinsic_index_4(q_1, n_1, q_2, n_2);
        let rank_diff = (b + 4 - a) % 4;
        let qd_x = (rotate90_by(q_2, n_2, rank_diff) + q_1) * 0.5;
        let qd_y = (rotate90_by(q_2_y, n_2, rank_diff) + q_1_y) * 0.5;
        let diff = info.edge_diff[e];
        let c =
            qd_x * (diff[0] as f64 * scale) + qd_y * (diff[1] as f64 * scale) + l.v[q1] - l.v[q2];
        let slot = ideal[p1].entry(p2).or_insert((0, Vec3::default()));
        slot.0 += 1;
        slot.1 = slot.1 + c;
    }

    let mut rows: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); num * 2];
    let mut b = vec![0.0; num * 2];
    for (m, targets) in ideal.iter().enumerate() {
        let v1 = index[m];
        for (&other, &(count, sum)) in targets {
            let v2 = index[other];
            let (q_1, q_2, n_1, n_2) = (l.q[v1], l.q[v2], l.n[v1], l.n[v2]);
            let weights = [q_2, n_2.cross(q_2), -q_1, -n_1.cross(q_1)];
            let vid = [other * 2, other * 2 + 1, m * 2, m * 2 + 1].map(|v| v as u32);
            let dis = sum / count as f64;
            for i in 0..4 {
                for j in 0..4 {
                    add_to(
                        &mut rows[vid[i] as usize],
                        vid[j],
                        weights[i].dot(weights[j]),
                    );
                }
                b[vid[i] as usize] += weights[i].dot(dis);
            }
        }
    }
    for (i, row) in rows.iter_mut().enumerate() {
        if row.is_empty() {
            row.insert(i as u32, 1.0);
        }
    }

    let mut x = vec![0.0; num * 2];
    for i in 0..num {
        let p = index[i];
        let (q, n) = (l.q[p], l.n[p]);
        let d = positions[i] - l.v[p];
        x[i * 2] = d.dot(q);
        x[i * 2 + 1] = d.dot(n.cross(q));
    }
    let x = solve_with_pull(&rows, &b, &x)?;

    for i in 0..vertex_count {
        let p = tree.index(i);
        let c = index[p];
        let (q, n) = (l.q[c], l.n[c]);
        l.o[i] = l.v[c] + q * x[p * 2] + n.cross(q) * x[p * 2 + 1];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_b::flip::fix_flip_hierarchy;
    use crate::route_b::integer::tests::through_max_flow;
    use crate::route_b::orient::tests::{cube, flat};
    use crate::route_b::subdivide::subdivide_edge_diff;
    use crate::route_b::Parametrizer;

    fn through_flip(mesh: (Vec<Vec3>, Vec<[u32; 3]>), faces: usize) -> (Parametrizer, EdgeInfo) {
        let (mut p, mut info, report) = through_max_flow(mesh, faces);
        assert!(report.full);
        subdivide_edge_diff(&mut p, &mut info, 1).unwrap();
        fix_flip_hierarchy(&mut info).unwrap();
        subdivide_edge_diff(&mut p, &mut info, 1).unwrap();
        (p, info)
    }

    #[test]
    fn a_well_posed_system_moves_by_about_the_pull() {
        let rows = vec![
            BTreeMap::from([(0, 4.0), (1, 1.0)]),
            BTreeMap::from([(0, 1.0), (1, 3.0)]),
        ];
        let x = solve_with_pull(&rows, &[6.0, 7.0], &[0.0, 0.0]).unwrap();
        assert!((x[0] - 1.0).abs() < 1e-7 && (x[1] - 2.0).abs() < 1e-7);
    }

    #[test]
    fn a_singular_system_keeps_the_current_value_along_its_null_direction() {
        let rows = vec![
            BTreeMap::from([(0, 1.0), (1, -1.0)]),
            BTreeMap::from([(0, -1.0), (1, 1.0)]),
        ];
        let x = solve_with_pull(&rows, &[-2.0, 2.0], &[10.0, 10.0]).unwrap();
        assert!((x[1] - x[0] - 2.0).abs() < 1e-6);
        assert!((x[0] + x[1] - 20.0).abs() < 1e-6);
    }

    #[test]
    fn on_a_flat_patch_every_group_lands_one_lattice_offset_from_its_neighbours() {
        let (mut p, info) = through_flip(flat(4), 200);
        let h = &mut p.hierarchy;
        optimize_positions_fixed(h, &info).unwrap();
        let l = &h.levels[0];
        let mut worst: f64 = 0.0;
        for (e, ev) in info.edge_values.iter().enumerate() {
            let (a, b) = (ev.x as usize, ev.y as usize);
            let (qa, ra) = (l.q[a], l.n[a].cross(l.q[a]));
            let d = info.edge_diff[e];
            let want = qa * (d[0] as f64 * h.scale) + ra * (d[1] as f64 * h.scale);
            let got = l.o[b] - l.o[a];
            let gap = (got - want).length();
            worst = worst.max(gap);
        }
        assert!(
            worst < 1e-4 * h.scale,
            "worst gap {worst} at scale {}",
            h.scale
        );
    }

    #[test]
    fn a_cube_solves_to_finite_positions() {
        let (mut p, info) = through_flip(cube(), 300);
        optimize_positions_fixed(&mut p.hierarchy, &info).unwrap();
        assert!(p.hierarchy.levels[0]
            .o
            .iter()
            .all(|o| o.x.is_finite() && o.y.is_finite() && o.z.is_finite()));
    }
}
