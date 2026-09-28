use super::dedge::INVALID;
use super::dset::{DisjointOrientTree, DisjointTree};
use super::field_math::{compat_orientation_extrinsic_index_4, rshift90, DEdge};
use super::flow::EcMaxFlow;
use super::pcg32::Pcg32;
use super::position::PositionSingularities;
use skp_core::geometry::Vec3;
use skp_core::progress::{CancelToken, Cancelled};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeInfo {
    pub edge_values: Vec<DEdge>,
    pub edge_diff: Vec<[i32; 2]>,
    pub face_edge_ids: Vec<[u32; 3]>,
    pub face_edge_orients: Vec<[i32; 3]>,
}

fn add(a: [i32; 2], b: [i32; 2]) -> [i32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

pub fn build_edge_info(
    faces: &[[u32; 3]],
    e2e: &[u32],
    pos: &PositionSingularities,
    singularities: &BTreeMap<u32, i32>,
) -> EdgeInfo {
    let mut info = EdgeInfo {
        face_edge_ids: vec![[INVALID; 3]; faces.len()],
        ..EdgeInfo::default()
    };
    for (i, face) in faces.iter().enumerate() {
        for k1 in 0..3 {
            let k2 = (k1 + 1) % 3;
            let (v1, v2) = (face[k1], face[k2]);
            let idx = pos.index[i];
            let diff2 = if v1 > v2 {
                rshift90([-idx[k1 * 2], -idx[k1 * 2 + 1]], pos.rank[i][k2])
            } else {
                rshift90([idx[k1 * 2], idx[k1 * 2 + 1]], pos.rank[i][k1])
            };
            let eid = e2e[i * 3 + k1];
            if info.face_edge_ids[i][k1] == INVALID {
                let id = info.edge_values.len() as u32;
                info.edge_values.push(DEdge::new(v1, v2));
                info.edge_diff.push(diff2);
                info.face_edge_ids[i][k1] = id;
                if eid != INVALID {
                    info.face_edge_ids[(eid / 3) as usize][(eid % 3) as usize] = id;
                }
            } else if !singularities.contains_key(&(i as u32)) {
                let id = info.face_edge_ids[(eid / 3) as usize][(eid % 3) as usize];
                info.edge_diff[id as usize] = diff2;
            }
        }
    }
    info
}

pub(crate) fn face_sum(info: &EdgeInfo, face: usize) -> [i32; 2] {
    (0..3).fold([0, 0], |acc, j| {
        let e = info.face_edge_ids[face][j] as usize;
        add(
            acc,
            rshift90(info.edge_diff[e], info.face_edge_orients[face][j]),
        )
    })
}

fn variables(info: &EdgeInfo) -> Vec<([i32; 2], i32)> {
    let mut vars = vec![([-1, -1], 0); info.edge_diff.len() * 2];
    let faces = info.face_edge_ids.iter().zip(&info.face_edge_orients);
    for (i, (ids, orients)) in faces.enumerate() {
        for (&id, &orient) in ids.iter().zip(orients) {
            let sign = rshift90([1, 1], orient);
            let eid = id as i32;
            let index = rshift90([eid * 2, eid * 2 + 1], orient);
            for k in 0..2 {
                let p = &mut vars[index[k].unsigned_abs() as usize];
                if p.0[0] == -1 {
                    p.0[0] = (i * 2 + k) as i32;
                } else {
                    p.0[1] = (i * 2 + k) as i32;
                }
                p.1 += sign[k];
            }
        }
    }
    vars
}

fn balance(
    info: &mut EdgeInfo,
    total_flows: &mut [i32],
    component_of_equation: impl Fn(usize) -> usize,
    rng: &mut Pcg32,
) {
    let vars = variables(info);
    let mut modified: [Vec<Vec<(usize, i32)>>; 2] = [
        vec![Vec::new(); total_flows.len()],
        vec![Vec::new(); total_flows.len()],
    ];
    for (i, var) in vars.iter().enumerate() {
        if !(var.0[1] == -1 || var.1 != 0) {
            continue;
        }
        let find = component_of_equation(var.0[0] as usize);
        let step = (var.1.abs() % 2) as usize;
        let current = info.edge_diff[i / 2][i % 2];
        if total_flows[find] > 0 {
            if var.1 > 0 && current > -1 {
                modified[step][find].push((i, -1));
            }
            if var.1 < 0 && current < 1 {
                modified[step][find].push((i, 1));
            }
        } else if total_flows[find] < 0 {
            if var.1 < 0 && current > -1 {
                modified[step][find].push((i, -1));
            }
            if var.1 > 0 && current < 1 {
                modified[step][find].push((i, 1));
            }
        }
    }
    for lists in &mut modified {
        for list in lists.iter_mut() {
            rng.shuffle(list);
        }
    }
    for (j, total) in total_flows.iter_mut().enumerate() {
        for (ii, lists) in modified.iter().enumerate() {
            if *total == 0 {
                continue;
            }
            let wanted = if ii == 0 {
                total.abs() / 2
            } else {
                total.abs()
            };
            let max_num = (wanted as usize).min(lists[j].len());
            let dir = if *total > 0 { -1 } else { 1 };
            for &(var, delta) in &lists[j][..max_num] {
                info.edge_diff[var / 2][var % 2] += delta;
                *total += if ii == 0 { 2 * dir } else { dir };
            }
        }
    }
}

fn edge_to_constraints(info: &EdgeInfo) -> (Vec<[i32; 4]>, Vec<i32>) {
    let mut constraints = vec![[-1, 0, -1, 0]; info.edge_diff.len() * 2];
    let mut initial = vec![0; info.face_edge_ids.len() * 2];
    let faces = info.face_edge_ids.iter().zip(&info.face_edge_orients);
    for (i, (ids, orients)) in faces.enumerate() {
        for (&id, &orient) in ids.iter().zip(orients) {
            let e = id as i32;
            let index = rshift90([e * 2 + 1, e * 2 + 2], orient);
            for (k, &value) in index.iter().enumerate() {
                let l = value.abs();
                let s = value / l;
                let ind = (l - 1) as usize;
                let equation = (i * 2 + k) as i32;
                if constraints[ind][0] == -1 {
                    constraints[ind][0] = equation;
                    constraints[ind][1] = s;
                } else {
                    constraints[ind][2] = equation;
                    constraints[ind][3] = s;
                }
                initial[equation as usize] += s * info.edge_diff[ind / 2][ind % 2];
            }
        }
    }
    (constraints, initial)
}

fn merge_across(tree: &mut DisjointOrientTree, info: &EdgeInfo, edge: (i32, i32)) {
    let (f0, f1) = ((edge.0 / 3) as usize, (edge.1 / 3) as usize);
    let orient1 = info.face_edge_orients[f0][(edge.0 % 3) as usize];
    let orient0 = (info.face_edge_orients[f1][(edge.1 % 3) as usize] + 2) % 4;
    tree.merge(f0, f1, orient0, orient1);
}

pub fn build_integer_constraints(
    faces: &[[u32; 3]],
    q: &[Vec3],
    n: &[Vec3],
    singularities: &BTreeMap<u32, i32>,
    info: &mut EdgeInfo,
    rng: &mut Pcg32,
) {
    let mut e2d = vec![(-1i32, -1i32); info.edge_diff.len()];
    info.face_edge_orients = Vec::with_capacity(faces.len());
    for (i, face) in faces.iter().enumerate() {
        let [v0, v1, v2] = face.map(|v| v as usize);
        let index1 = compat_orientation_extrinsic_index_4(q[v0], n[v0], q[v1], n[v1]);
        let index2 = compat_orientation_extrinsic_index_4(q[v0], n[v0], q[v2], n[v2]);
        let rank1 = (index1.0 - index1.1 + 4) % 4;
        let rank2 = (index2.0 - index2.1 + 4) % 4;
        let o0 = if v1 < v0 { (rank1 + 2) % 4 } else { 0 };
        let o1 = if v2 < v1 { (rank2 + 2) % 4 } else { rank1 };
        let o2 = if v2 < v0 { rank2 } else { 2 };
        info.face_edge_orients.push([o0, o1, o2]);
        for j in 0..3 {
            let eid = info.face_edge_ids[i][j] as usize;
            let de = (i * 3 + j) as i32;
            if e2d[eid].0 == -1 {
                e2d[eid].0 = de;
            } else {
                e2d[eid].1 = de;
            }
        }
    }

    let mut tree = DisjointOrientTree::new(faces.len());
    for &edge in &e2d {
        if edge.0 == -1 || edge.1 == -1 {
            continue;
        }
        let (f0, f1) = ((edge.0 / 3) as u32, (edge.1 / 3) as u32);
        if singularities.contains_key(&f0) || singularities.contains_key(&f1) {
            continue;
        }
        merge_across(&mut tree, info, edge);
    }
    for &f in singularities.keys() {
        for i in 0..3 {
            let edge = e2d[info.face_edge_ids[f as usize][i] as usize];
            if edge.0 == -1 || edge.1 == -1 {
                continue;
            }
            merge_across(&mut tree, info, edge);
        }
    }
    for (i, orients) in info.face_edge_orients.iter_mut().enumerate() {
        let turn = tree.orient(i);
        for o in orients.iter_mut() {
            *o = (*o + turn) % 4;
        }
    }

    let mut colour = vec![usize::MAX; faces.len()];
    let mut components = 0;
    for start in 0..faces.len() {
        if colour[start] != usize::MAX {
            continue;
        }
        colour[start] = components;
        let mut queue = VecDeque::from([start]);
        while let Some(v) = queue.pop_front() {
            for i in 0..3 {
                let (d1, d2) = e2d[info.face_edge_ids[v][i] as usize];
                if d1 == -1 || d2 == -1 {
                    continue;
                }
                let a = info.face_edge_orients[(d1 / 3) as usize][(d1 % 3) as usize];
                let b = info.face_edge_orients[(d2 / 3) as usize][(d2 % 3) as usize];
                if (a - b + 4).abs() % 4 != 2 {
                    continue;
                }
                for f in [(d1 / 3) as usize, (d2 / 3) as usize] {
                    if colour[f] == usize::MAX {
                        colour[f] = components;
                        queue.push_back(f);
                    }
                }
            }
        }
        components += 1;
    }
    let mut total_flows = vec![0; components];
    for (i, &c) in colour.iter().enumerate() {
        let d = face_sum(info, i);
        total_flows[c] += d[0] + d[1];
    }
    balance(info, &mut total_flows, |equation| colour[equation / 2], rng);

    let (constraints, _) = edge_to_constraints(info);
    let mut groups = DisjointTree::new(faces.len() * 2);
    for c in &constraints {
        if c[0] == -1 || c[2] == -1 {
            continue;
        }
        if c[1] == -c[3] {
            groups.merge(c[0] as usize, c[2] as usize);
        }
    }
    groups.build_compact_parent();
    let mut total_flows = vec![0; groups.compact_num()];
    for i in 0..faces.len() {
        let d = face_sum(info, i);
        for (j, dj) in d.into_iter().enumerate() {
            total_flows[groups.index(i * 2 + j)] += dj;
        }
    }
    balance(
        info,
        &mut total_flows,
        |equation| groups.index(equation),
        rng,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowReport {
    pub supply: i32,
    pub flow: i32,
    pub rounds: u32,
    pub full: bool,
}

pub fn compute_max_flow(
    info: &mut EdgeInfo,
    cancel: &CancelToken,
) -> Result<FlowReport, Cancelled> {
    let mut edge_capacity = 2;
    let mut report = FlowReport {
        supply: 0,
        flow: 0,
        rounds: 0,
        full: false,
    };
    loop {
        let (constraints, initial) = edge_to_constraints(info);
        let mut arcs: Vec<(i32, i32, i32, i32)> = Vec::new();
        for (i, c) in constraints.iter().enumerate() {
            if c[0] == -1 || c[2] == -1 || c[1] != -c[3] {
                continue;
            }
            let (mut v1, mut v2) = (c[0], c[2]);
            if c[1] < 0 {
                std::mem::swap(&mut v1, &mut v2);
            }
            arcs.push((v1, v2, info.edge_diff[i / 2][i % 2], i as i32));
        }
        let sink = initial.len() as i32;
        let mut supply = 0;
        for (i, &value) in initial.iter().enumerate() {
            if value > 0 {
                arcs.push((-1, i as i32, value, -1));
                supply += value;
            } else if value < 0 {
                arcs.push((i as i32, sink, -value, -1));
            }
        }

        let mut solver = EcMaxFlow::new(initial.len() + 2);
        for &(a, b, c, variable) in &arcs {
            let (v1, v2) = ((a + 1) as u32, (b + 1) as u32);
            if variable == -1 {
                solver.add_edge(v1, v2, c, 0, -1);
            } else {
                solver.add_edge(
                    v1,
                    v2,
                    (c + edge_capacity).max(0),
                    (-c + edge_capacity).max(0),
                    variable,
                );
            }
        }
        let flow = solver.compute(cancel)?;
        solver.apply_to(&mut info.edge_diff);
        report = FlowReport {
            supply: if report.rounds == 0 {
                supply
            } else {
                report.supply
            },
            flow: report.flow + flow,
            rounds: report.rounds + 1,
            full: flow == supply,
        };
        if report.full || report.rounds == 10 {
            return Ok(report);
        }
        edge_capacity += 1;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::route_b::orient::tests::{cube, flat, initialised};
    use crate::route_b::orient::{optimize_orientations, orientation_singularities};
    use crate::route_b::position::{optimize_positions, position_singularities};
    use crate::route_b::Parametrizer;

    pub(crate) fn through_max_flow(
        mesh: (Vec<Vec3>, Vec<[u32; 3]>),
        faces: usize,
    ) -> (Parametrizer, EdgeInfo, FlowReport) {
        let mut p = initialised(mesh, faces);
        let h = &mut p.hierarchy;
        optimize_orientations(h);
        let sing = orientation_singularities(h);
        optimize_positions(h);
        let pos = position_singularities(h);
        let mut info = build_edge_info(&h.faces, &h.e2e, &pos, &sing);
        let l = &h.levels[0];
        build_integer_constraints(
            &h.faces,
            &l.q,
            &l.n,
            &sing,
            &mut info,
            &mut Pcg32::seeded(5, 2),
        );
        let report = compute_max_flow(&mut info, &CancelToken::new()).unwrap();
        (p, info, report)
    }

    fn integer_stage(
        mesh: (Vec<Vec3>, Vec<[u32; 3]>),
        faces: usize,
    ) -> (EdgeInfo, FlowReport, usize) {
        let (p, info, report) = through_max_flow(mesh, faces);
        let count = p.hierarchy.faces.len();
        (info, report, count)
    }

    #[test]
    fn twin_faces_share_one_edge_id() {
        let faces = [[0, 1, 2], [0, 2, 3]];
        let e2e = [INVALID, INVALID, 3, 2, INVALID, INVALID];
        let pos = PositionSingularities {
            singular: BTreeMap::new(),
            rank: vec![[0; 3]; 2],
            index: vec![[1, 0, 0, 2, 0, 0], [0, 0, 0, 0, 0, 0]],
        };
        let info = build_edge_info(&faces, &e2e, &pos, &BTreeMap::new());
        assert_eq!(info.edge_values.len(), 5);
        assert_eq!(info.face_edge_ids[0][2], info.face_edge_ids[1][0]);
        assert_eq!(info.edge_values[0], DEdge::new(0, 1));
        assert_eq!(info.edge_diff[0], [1, 0]);
        assert_eq!(info.edge_diff[1], [0, 2]);
    }

    #[test]
    fn a_flat_patch_reaches_full_flow_and_every_face_closes() {
        let (info, report, faces) = integer_stage(flat(4), 200);
        assert!(report.full, "{report:?}");
        for f in 0..faces {
            assert_eq!(face_sum(&info, f), [0, 0], "face {f}");
        }
    }

    #[test]
    fn a_cube_reaches_full_flow_and_every_face_closes() {
        let (info, report, faces) = integer_stage(cube(), 300);
        assert!(report.full, "{report:?}");
        for f in 0..faces {
            assert_eq!(face_sum(&info, f), [0, 0], "face {f}");
        }
    }
}
