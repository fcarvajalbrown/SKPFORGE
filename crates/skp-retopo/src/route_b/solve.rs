use super::dedge::INVALID;
use super::dset::DisjointTree;
use super::extract::QuadMesh;
use super::field_math::{compat_orientation_extrinsic_index_4, normalized, rotate90_by, rshift90};
use super::hierarchy::{Hierarchy, Level};
use super::integer::EdgeInfo;
use super::sparse::{Cholesky, SymmetricMatrix};
use super::Parametrizer;
use crate::error::RetopoError;
use skp_core::geometry::Vec3;
use skp_core::progress::CancelToken;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

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

#[allow(clippy::approx_constant)]
const UPSTREAM_PI: f64 = 3.141592654;
const DYNAMIC_ITERATIONS: usize = 10;

fn angle_axis(angle: f64, axis: Vec3, d: Vec3) -> Vec3 {
    let sin_axis = axis * angle.sin();
    let c = angle.cos();
    let cos1_axis = axis * (1.0 - c);
    let xy = cos1_axis.x * axis.y;
    let xz = cos1_axis.x * axis.z;
    let yz = cos1_axis.y * axis.z;
    let m = [
        [cos1_axis.x * axis.x + c, xy - sin_axis.z, xz + sin_axis.y],
        [xy + sin_axis.z, cos1_axis.y * axis.y + c, yz - sin_axis.x],
        [xz - sin_axis.y, yz + sin_axis.x, cos1_axis.z * axis.z + c],
    ];
    Vec3::new(
        m[0][0] * d.x + m[0][1] * d.y + m[0][2] * d.z,
        m[1][0] * d.x + m[1][1] * d.y + m[1][2] * d.z,
        m[2][0] * d.x + m[2][1] * d.y + m[2][2] * d.z,
    )
}

fn next4(e: u32) -> u32 {
    e / 4 * 4 + (e + 1) % 4
}

fn prev4(e: u32) -> u32 {
    e / 4 * 4 + (e + 3) % 4
}

struct Targets {
    o2e: BTreeMap<(u32, u32), u32>,
    diffs: Vec<Vec3>,
    counts: Vec<i32>,
}

fn dynamic_targets(p: &Parametrizer, info: &EdgeInfo, quads: &QuadMesh) -> Targets {
    let l = &p.hierarchy.levels[0];
    let faces = &p.hierarchy.faces;
    let scale = p.hierarchy.scale;
    let mut o2e = BTreeMap::new();
    for (i, f) in quads.faces.iter().enumerate() {
        for j in 0..4 {
            o2e.insert((f[j], f[(j + 1) % 4]), (i * 4 + j) as u32);
        }
    }
    let mut v2o: Vec<Vec<u32>> = vec![Vec::new(); l.v.len()];
    for (i, set) in quads.vset.iter().enumerate() {
        for &v in set {
            v2o[v as usize].push(i as u32);
        }
    }
    let mut diffs = vec![Vec3::default(); quads.faces.len() * 4];
    let mut counts = vec![0i32; quads.faces.len() * 4];
    for (i, f) in faces.iter().enumerate() {
        for j in 0..3 {
            let (v1, v2) = (f[j] as usize, f[(j + 1) % 3] as usize);
            let e = info.face_edge_ids[i][j] as usize;
            if v1 as u32 != info.edge_values[e].x {
                continue;
            }
            let d = info.edge_diff[e];
            if d[0].abs() + d[1].abs() != 1 || v2o[v1].len() > 1 || v2o[v2].len() > 1 {
                continue;
            }
            for &o1 in &v2o[v1] {
                for &o2 in &v2o[v2] {
                    let Some(&dedge) = o2e.get(&(o1, o2)) else {
                        continue;
                    };
                    let (q_1, q_2, n_1, n_2) = (l.q[v1], l.q[v2], l.n[v1], l.n[v2]);
                    let (a, b) = compat_orientation_extrinsic_index_4(q_1, n_1, q_2, n_2);
                    let rank_diff = (b + 4 - a) % 4;
                    let qd_x = (rotate90_by(q_2, n_2, rank_diff) + q_1) * 0.5;
                    let qd_y = (rotate90_by(n_2.cross(q_2), n_2, rank_diff) + n_1.cross(q_1)) * 0.5;
                    let c = qd_x * (d[0] as f64 * scale) + qd_y * (d[1] as f64 * scale);
                    counts[dedge as usize] += 1;
                    diffs[dedge as usize] = diffs[dedge as usize] + c;
                    if let Some(&back) = o2e.get(&(o2, o1)) {
                        counts[back as usize] += 1;
                        diffs[back as usize] = diffs[back as usize] - c;
                    }
                }
            }
        }
    }
    for (i, f) in faces.iter().enumerate() {
        let d1 = rshift90(
            info.edge_diff[info.face_edge_ids[i][0] as usize],
            info.face_edge_orients[i][0],
        );
        let d2 = rshift90(
            info.edge_diff[info.face_edge_ids[i][1] as usize],
            info.face_edge_orients[i][1],
        );
        if d1[0] * d2[1] - d1[1] * d2[0] >= 0 {
            continue;
        }
        for j in 0..3 {
            let (v1, v2) = (f[j] as usize, f[(j + 1) % 3] as usize);
            for &o1 in &v2o[v1] {
                for &o2 in &v2o[v2] {
                    if let Some(&dedge) = o2e.get(&(o1, o2)) {
                        counts[dedge as usize] = 0;
                        diffs[dedge as usize] = Vec3::default();
                    }
                }
            }
        }
    }
    for (d, c) in diffs.iter_mut().zip(counts.iter_mut()) {
        if *c != 0 {
            *d = *d / *c as f64;
            *c = 1;
        }
    }
    Targets { o2e, diffs, counts }
}

type Rings = (Vec<VecDeque<u32>>, Vec<VecDeque<u32>>);

fn rings(quads: &QuadMesh) -> Rings {
    let n = quads.o.len();
    let mut links = vec![VecDeque::new(); n];
    let mut dedges = vec![VecDeque::new(); n];
    let at = |d: u32, k: u32| quads.faces[(d / 4) as usize][((d + k) % 4) as usize];
    for i in 0..n {
        let deid0 = quads.v2e[i];
        if deid0 == INVALID {
            continue;
        }
        let mut deid = deid0;
        loop {
            links[i].push_back(at(deid, 1));
            dedges[i].push_back(deid);
            deid = quads.e2e[prev4(deid) as usize];
            if deid == INVALID || deid == deid0 {
                break;
            }
        }
        if deid == INVALID {
            deid = deid0;
            loop {
                deid = quads.e2e[deid as usize];
                if deid == INVALID {
                    break;
                }
                deid = next4(deid);
                links[i].push_front(at(deid, 1));
                dedges[i].push_front(deid);
            }
        }
    }
    (links, dedges)
}

fn find_nearest(
    quads: &mut QuadMesh,
    l: &Level,
    adj: &[Vec<u32>],
    dedges: &[VecDeque<u32>],
    vind: &mut [usize],
    diffs: &mut [Vec3],
) {
    let cos10 = (10.0 / 180.0 * UPSTREAM_PI).cos();
    for i in 0..quads.o.len() {
        if vind[i] == usize::MAX {
            let mut min_dis = 1e30;
            let mut min_ind = usize::MAX;
            for &v in &quads.vset[i] {
                let d = l.v[v as usize] - quads.o[i];
                if d.dot(d) < min_dis {
                    min_dis = d.dot(d);
                    min_ind = v as usize;
                }
            }
            if min_ind != usize::MAX {
                vind[i] = min_ind;
                let x = (quads.o[i] - l.v[min_ind]).dot(l.n[min_ind]);
                quads.o[i] = quads.o[i] - l.n[min_ind] * x;
            }
            continue;
        }
        let mut current = vind[i];
        let n = l.n[current];
        let d = quads.o[i] - l.v[current];
        let mut current_dis = d.dot(d);
        loop {
            let mut next = usize::MAX;
            for &v in &adj[current] {
                if l.n[v as usize].dot(n) < cos10 {
                    continue;
                }
                let d = quads.o[i] - l.v[v as usize];
                if d.dot(d) < current_dis {
                    current_dis = d.dot(d);
                    next = v as usize;
                }
            }
            if next == usize::MAX {
                break;
            }
            let (n1, n2) = (l.n[current], l.n[next]);
            let axis = n1.cross(n2);
            let angle = axis.length().atan2(n1.dot(n2));
            for &e in &dedges[i] {
                diffs[e as usize] = angle_axis(angle, axis, diffs[e as usize]);
            }
            current = next;
        }
        vind[i] = current;
    }
}

fn compute_distance(
    quads: &QuadMesh,
    o2e: &BTreeMap<(u32, u32), u32>,
    dedges: &[VecDeque<u32>],
    vind: &[usize],
    l: &Level,
    diffs: &mut [Vec3],
    counts: &mut [i32],
) {
    let mut unobserved: BTreeSet<u32> = o2e
        .iter()
        .filter(|(_, &e)| counts[e as usize] == 0)
        .map(|(&(a, _), _)| a)
        .collect();
    loop {
        let mut update = false;
        let mut observed = BTreeSet::new();
        for &p in &unobserved {
            let edges: Vec<u32> = dedges[p as usize].iter().copied().collect();
            let mut observations: Vec<bool> =
                edges.iter().map(|&e| counts[e as usize] != 0).collect();
            if observations.iter().filter(|&&o| o).count() <= 1 {
                continue;
            }
            update = true;
            observed.insert(p);
            let len = edges.len();
            for i in 0..len {
                if observations[i] {
                    continue;
                }
                let mut interp: VecDeque<usize> = VecDeque::new();
                let mut j = i;
                while !observations[j] {
                    interp.push_front(j);
                    j = if j == 0 { len - 1 } else { j - 1 };
                }
                j = (i + 1) % len;
                while !observations[j] {
                    interp.push_back(j);
                    j = (j + 1) % len;
                }
                let front = *interp.front().expect("holds i");
                let back = *interp.back().expect("holds i");
                let dl = diffs[edges[(front + len - 1) % len] as usize];
                let lenl = dl.length();
                let dr = diffs[edges[(back + 1) % len] as usize];
                let lenr = dr.length();
                let (dl, dr) = (dl / lenl, dr / lenr);
                let n = normalized(dl.cross(dr));
                let mut angle = dl.cross(dr).length().atan2(dl.dot(dr));
                if angle < 0.0 {
                    angle += 2.0 * UPSTREAM_PI;
                }
                let nc = l.n[vind[p as usize]];
                if n.dot(nc) < 0.0 {
                    angle = 2.0 * UPSTREAM_PI - angle;
                }
                let steps = (interp.len() + 1) as f64;
                let step = (lenr - lenl) / steps;
                angle /= steps;
                let dlp = normalized(nc.cross(dl));
                for (t, &q) in interp.iter().enumerate() {
                    let t = (t + 1) as f64;
                    observations[q] = true;
                    let ad = angle * t;
                    let e = edges[q] as usize;
                    let re = quads.e2e[e];
                    counts[e] = 2;
                    diffs[e] = (dl * ad.cos() + dlp * ad.sin()) * (lenl + step * t);
                    if re != INVALID {
                        counts[re as usize] = 2;
                        diffs[re as usize] = -diffs[e];
                    }
                }
            }
        }
        if !update {
            return;
        }
        for p in observed {
            unobserved.remove(&p);
        }
    }
}

fn smooth_irregular(quads: &mut QuadMesh, dedges: &[VecDeque<u32>], uncertain: &BTreeSet<u32>) {
    for _ in 0..5 {
        for (i, ring) in dedges.iter().enumerate() {
            let regular = ring.len() == 4 && !uncertain.contains(&(i as u32));
            if regular || ring.is_empty() {
                continue;
            }
            let v0 = quads.o[i];
            let mut n = Vec3::default();
            let mut v = Vec3::default();
            for &e in ring {
                let f = quads.faces[(e / 4) as usize];
                let v1 = quads.o[f[((e + 1) % 4) as usize] as usize];
                let v2 = quads.o[f[((e + 3) % 4) as usize] as usize];
                n = n + (v1 - v0).cross(v2 - v0);
                v = v + v1;
            }
            let n = normalized(n);
            let mut offset = v / ring.len() as f64 - v0;
            offset = offset - n * offset.dot(n);
            quads.o[i] = quads.o[i] + offset;
        }
    }
}

pub fn optimize_positions_dynamic(
    p: &Parametrizer,
    info: &EdgeInfo,
    quads: &mut QuadMesh,
    cancel: &CancelToken,
) -> Result<(), RetopoError> {
    let Targets {
        o2e,
        mut diffs,
        mut counts,
    } = dynamic_targets(p, info, quads);
    let l = &p.hierarchy.levels[0];
    let mut uncertain = BTreeSet::new();
    for (&(a, b), &e) in &o2e {
        if counts[e as usize] == 0 {
            uncertain.insert(a);
            uncertain.insert(b);
        }
    }
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); l.v.len()];
    for f in &p.hierarchy.faces {
        for j in 0..3 {
            adj[f[j] as usize].push(f[(j + 1) % 3]);
        }
    }
    let (links, dedges) = rings(quads);
    let count = quads.o.len();
    let mut vind = vec![usize::MAX; count];

    for iter in 0..DYNAMIC_ITERATIONS {
        cancel.check()?;
        find_nearest(quads, l, &adj, &dedges, &mut vind, &mut diffs);
        compute_distance(quads, &o2e, &dedges, &vind, l, &mut diffs, &mut counts);

        let frame = |i: usize| {
            let v = vind[i];
            (l.q[v], l.n[v], l.v[v])
        };
        let mut x = vec![0.0; count * 2];
        for i in 0..count {
            let (q, n, v) = frame(i);
            let d = quads.o[i] - v;
            x[i * 2] = d.dot(q);
            x[i * 2 + 1] = d.dot(n.cross(q));
        }
        let mut rows: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); count * 2];
        let mut b = vec![0.0; count * 2];
        for i in 0..count {
            let (qx, ni, vi) = frame(i);
            let qy = ni.cross(qx);
            for &j in &links[i] {
                let j = j as usize;
                let (qx2, nj, vj) = frame(j);
                let qy2 = nj.cross(qx2);
                let de = o2e[&(i as u32, j as u32)];
                let c = diffs[de as usize] - (vj - vi);
                let vid = [j * 2, j * 2 + 1, i * 2, i * 2 + 1].map(|v| v as u32);
                let weights = [qx2, qy2, -qx, -qy];
                for a in 0..4 {
                    for w in 0..4 {
                        add_to(
                            &mut rows[vid[a] as usize],
                            vid[w],
                            weights[a].dot(weights[w]),
                        );
                    }
                    b[vid[a] as usize] += weights[a].dot(c);
                }
            }
        }
        for (i, row) in rows.iter_mut().enumerate() {
            if row.is_empty() {
                row.insert(i as u32, 1.0);
                b[i] = x[i];
            }
        }
        let solved = solve_with_pull(&rows, &b, &x)?;
        for i in 0..count {
            let (q, n, v) = frame(i);
            quads.o[i] = v + q * solved[i * 2] + n.cross(q) * solved[i * 2 + 1];
        }
        if iter + 1 == DYNAMIC_ITERATIONS {
            smooth_irregular(quads, &dedges, &uncertain);
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::route_b::flip::fix_flip_hierarchy;
    use crate::route_b::integer::tests::through_max_flow;
    use crate::route_b::orient::tests::{cube, flat};
    use crate::route_b::subdivide::subdivide_edge_diff;
    use crate::route_b::Parametrizer;

    pub(crate) fn through_flip(
        mesh: (Vec<Vec3>, Vec<[u32; 3]>),
        faces: usize,
    ) -> (Parametrizer, EdgeInfo) {
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

    fn through_dynamic(mesh: (Vec<Vec3>, Vec<[u32; 3]>), faces: usize) -> (f64, QuadMesh) {
        let (mut p, mut info) = through_flip(mesh, faces);
        optimize_positions_fixed(&mut p.hierarchy, &info).unwrap();
        let mut quads = crate::route_b::extract::advanced_extract_quad(&p, &mut info)
            .unwrap()
            .quads;
        crate::route_b::valence::fix_valence(&mut quads);
        optimize_positions_dynamic(&p, &info, &mut quads, &CancelToken::new()).unwrap();
        (p.hierarchy.scale, quads)
    }

    fn mean_edge(quads: &QuadMesh) -> f64 {
        let mut total = 0.0;
        for f in &quads.faces {
            for j in 0..4 {
                total += (quads.o[f[(j + 1) % 4] as usize] - quads.o[f[j] as usize]).length();
            }
        }
        total / (quads.faces.len() * 4) as f64
    }

    #[test]
    fn angle_axis_follows_eigen_even_for_an_axis_that_is_not_unit() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let half_pi = std::f64::consts::FRAC_PI_2;
        let r = angle_axis(half_pi, Vec3::new(0.0, 0.0, 1.0), x);
        assert!((r - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-15);
        let r = angle_axis(half_pi, Vec3::new(0.0, 0.0, 0.5), x);
        assert!((r - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-15);
    }

    #[test]
    fn the_dynamic_solve_keeps_a_flat_patch_flat_with_cells_near_the_target_size() {
        let (scale, quads) = through_dynamic(flat(4), 200);
        assert!(quads.o.iter().all(|o| o.z.abs() < 1e-9));
        let mean = mean_edge(&quads);
        assert!(
            (mean / scale - 1.0).abs() < 0.2,
            "mean edge {mean}, scale {scale}"
        );
    }

    #[test]
    fn the_dynamic_solve_keeps_a_cube_on_its_surface_with_cells_near_the_target_size() {
        let (scale, quads) = through_dynamic(cube(), 300);
        for o in &quads.o {
            let reach = o.x.abs().max(o.y.abs()).max(o.z.abs());
            assert!((reach - 1.0).abs() < 0.05, "vertex {o:?} is off the cube");
        }
        let mean = mean_edge(&quads);
        assert!(
            (mean / scale - 1.0).abs() < 0.2,
            "mean edge {mean}, scale {scale}"
        );
    }
}
