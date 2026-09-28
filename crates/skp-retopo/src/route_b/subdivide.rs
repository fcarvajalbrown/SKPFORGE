use super::dedge::{dedge_next, dedge_prev, INVALID};
use super::field_math::{compat_orientation_extrinsic_index_4, rshift90, DEdge};
use super::integer::EdgeInfo;
use super::Parametrizer;
use crate::error::RetopoError;
use skp_core::geometry::Vec3;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Debug, Clone, Copy)]
struct Entry {
    length: f64,
    edge: u32,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Entry) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Entry) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Entry {
    fn cmp(&self, other: &Entry) -> Ordering {
        self.length
            .total_cmp(&other.length)
            .then(self.edge.cmp(&other.edge))
    }
}

fn squared_length(positions: &[Vec3], a: u32, b: u32) -> f64 {
    let d = positions[a as usize] - positions[b as usize];
    d.dot(d)
}

fn too_long(length: f64, max_squared: f64, rho_a: f64, rho_b: f64) -> bool {
    length > max_squared || length > (max_squared * 0.75).max(rho_a.min(rho_b))
}

fn link(e2e: &mut [u32], a: u32, b: u32) {
    e2e[a as usize] = b;
    if b != INVALID {
        e2e[b as usize] = a;
    }
}

pub struct Subdivided {
    pub faces: Vec<[u32; 3]>,
    pub positions: Vec<Vec3>,
    pub rho: Vec<f64>,
    pub e2e: Vec<u32>,
    pub splits: usize,
}

pub fn subdivide(
    faces: Vec<[u32; 3]>,
    positions: Vec<Vec3>,
    rho: Vec<f64>,
    e2e: Vec<u32>,
    non_manifold: &[bool],
    max_length: f64,
) -> Subdivided {
    let mut s = Subdivided {
        faces,
        positions,
        rho,
        e2e,
        splits: 0,
    };
    let max_squared = max_length * max_length;
    let mut queue = BinaryHeap::new();

    for i in 0..s.e2e.len() as u32 {
        let face = s.faces[(i / 3) as usize];
        let (v0, v1) = (face[(i % 3) as usize], face[((i + 1) % 3) as usize]);
        if non_manifold[v0 as usize] || non_manifold[v1 as usize] {
            continue;
        }
        let length = squared_length(&s.positions, v0, v1);
        if too_long(length, max_squared, s.rho[v0 as usize], s.rho[v1 as usize]) {
            let other = s.e2e[i as usize];
            if other == INVALID || other > i {
                queue.push(Entry { length, edge: i });
            }
        }
    }

    while let Some(Entry { length, edge: e0 }) = queue.pop() {
        let e1 = s.e2e[e0 as usize];
        let is_boundary = e1 == INVALID;
        let f0 = (e0 / 3) as usize;
        let at = |e: u32, k: u32| ((e + k) % 3) as usize;
        let (v0, v0p, v1) = (
            s.faces[f0][at(e0, 0)],
            s.faces[f0][at(e0, 2)],
            s.faces[f0][at(e0, 1)],
        );
        if squared_length(&s.positions, v0, v1) != length {
            continue;
        }
        let f1 = if is_boundary {
            usize::MAX
        } else {
            (e1 / 3) as usize
        };
        let v1p = if is_boundary {
            INVALID
        } else {
            s.faces[f1][at(e1, 2)]
        };

        let vn = s.positions.len() as u32;
        s.splits += 1;
        s.positions
            .push((s.positions[v0 as usize] + s.positions[v1 as usize]) * 0.5);
        s.rho.push(0.5 * s.rho[v1 as usize]);

        let f2 = if is_boundary {
            usize::MAX
        } else {
            s.faces.push([0; 3]);
            s.faces.len() - 1
        };
        s.faces.push([0; 3]);
        let f3 = s.faces.len() - 1;
        s.e2e.resize(s.faces.len() * 3, INVALID);

        s.faces[f0] = [vn, v0p, v0];
        if !is_boundary {
            s.faces[f1] = [vn, v0, v1p];
            s.faces[f2] = [vn, v1p, v1];
        }
        s.faces[f3] = [vn, v1, v0p];

        let e0p = s.e2e[dedge_prev(e0, 3) as usize];
        let e0n = s.e2e[dedge_next(e0, 3) as usize];
        let (f0, f3) = (f0 as u32, f3 as u32);
        let e2e = &mut s.e2e;
        link(e2e, 3 * f0, 3 * f3 + 2);
        link(e2e, 3 * f0 + 1, e0p);
        link(e2e, 3 * f3 + 1, e0n);
        if is_boundary {
            link(e2e, 3 * f0 + 2, INVALID);
            link(e2e, 3 * f3, INVALID);
        } else {
            let (f1, f2) = (f1 as u32, f2 as u32);
            let e1p = e2e[dedge_prev(e1, 3) as usize];
            let e1n = e2e[dedge_next(e1, 3) as usize];
            link(e2e, 3 * f0 + 2, 3 * f1);
            link(e2e, 3 * f1 + 1, e1n);
            link(e2e, 3 * f1 + 2, 3 * f2);
            link(e2e, 3 * f2 + 1, e1p);
            link(e2e, 3 * f2 + 2, 3 * f3);
        }

        let mut schedule = |f: u32| {
            for i in 0..3 {
                let face = s.faces[f as usize];
                let (a, b) = (face[i], face[(i + 1) % 3]);
                let length = squared_length(&s.positions, a, b);
                if too_long(length, max_squared, s.rho[a as usize], s.rho[b as usize]) {
                    queue.push(Entry {
                        length,
                        edge: f * 3 + i as u32,
                    });
                }
            }
        };
        schedule(f0);
        if !is_boundary {
            schedule(f2 as u32);
            schedule(f1 as u32);
        }
        schedule(f3);
    }
    s
}

#[derive(Debug, Clone, Copy)]
struct EdgeLink {
    max_len: i32,
    seq: u64,
    id: u32,
    length: f64,
}

impl PartialEq for EdgeLink {
    fn eq(&self, other: &EdgeLink) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for EdgeLink {}

impl PartialOrd for EdgeLink {
    fn partial_cmp(&self, other: &EdgeLink) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EdgeLink {
    fn cmp(&self, other: &EdgeLink) -> Ordering {
        self.max_len
            .cmp(&other.max_len)
            .then(other.seq.cmp(&self.seq))
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct FaceOrient {
    orient: i32,
    d: [i32; 3],
    q: Vec3,
    n: Vec3,
}

fn sub(a: [i32; 2], b: [i32; 2]) -> [i32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn plus(a: [i32; 2], b: [i32; 2]) -> [i32; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn neg(a: [i32; 2]) -> [i32; 2] {
    [-a[0], -a[1]]
}

fn exceeds(diff: [i32; 2], max_len: i32) -> bool {
    diff[0].abs() > max_len || diff[1].abs() > max_len
}

struct EdgeSplit<'a> {
    p: &'a mut Parametrizer,
    info: &'a mut EdgeInfo,
    spaces: Vec<FaceOrient>,
    diffs: Vec<[i32; 2]>,
    queue: BinaryHeap<EdgeLink>,
    seq: u64,
}

impl EdgeSplit<'_> {
    fn analyze_orient(&mut self, f0: usize, d: [i32; 3]) {
        let face = self.p.hierarchy.faces[f0];
        let l = &self.p.hierarchy.levels[0];
        for j in 0..3 {
            let mut orient = self.spaces[f0].orient + d[j];
            let v = face[j].min(face[(j + 1) % 3]);
            let (a, b) = compat_orientation_extrinsic_index_4(
                l.q[v as usize],
                l.n[v as usize],
                self.spaces[f0].q,
                self.spaces[f0].n,
            );
            if face[j] != v {
                orient += 2;
            }
            self.info.face_edge_orients[f0][j] = (orient + b - a + 4) % 4;
        }
        self.spaces[f0].d = d;
        for j in 0..3 {
            let eid = self.info.face_edge_ids[f0][j] as usize;
            let orient = self.info.face_edge_orients[f0][j];
            self.info.edge_diff[eid] = rshift90(self.diffs[f0 * 3 + j], (4 - orient) % 4);
        }
    }

    fn fix_orient(&mut self, f0: usize) {
        for j in 0..3 {
            let diff = self.info.edge_diff[self.info.face_edge_ids[f0][j] as usize];
            let target = self.diffs[f0 * 3 + j];
            let current = self.info.face_edge_orients[f0][j];
            if rshift90(diff, current) != target {
                let mut orient = 0;
                while orient < 4 && rshift90(diff, orient) != target {
                    orient += 1;
                }
                self.spaces[f0].d[j] = (self.spaces[f0].d[j] + orient - current) % 4;
                self.info.face_edge_orients[f0][j] = orient;
            }
        }
    }

    fn schedule(&mut self, id: u32, max_len: i32) {
        let diff = self.diffs[id as usize];
        if !exceeds(diff, max_len) {
            return;
        }
        let face = self.p.hierarchy.faces[(id / 3) as usize];
        let i = (id % 3) as usize;
        let v = &self.p.hierarchy.levels[0].v;
        self.seq += 1;
        self.queue.push(EdgeLink {
            max_len: diff[0].abs().max(diff[1].abs()),
            seq: self.seq,
            id,
            length: squared_length(v, face[(i + 1) % 3], face[i]),
        });
    }

    fn new_face(&mut self) -> usize {
        self.p.hierarchy.faces.push([0; 3]);
        self.info.face_edge_ids.push([0; 3]);
        self.info.face_edge_orients.push([0; 3]);
        self.spaces.push(FaceOrient::default());
        self.diffs.extend([[0, 0]; 3]);
        self.p.hierarchy.e2e.extend([INVALID; 3]);
        self.p.hierarchy.faces.len() - 1
    }
}

fn face_spaces(p: &Parametrizer, info: &EdgeInfo) -> Vec<FaceOrient> {
    let l = &p.hierarchy.levels[0];
    p.hierarchy
        .faces
        .iter()
        .enumerate()
        .map(|(i, face)| {
            let mut orient = FaceOrient {
                q: l.q[face[0] as usize],
                n: l.n[face[0] as usize],
                ..FaceOrient::default()
            };
            let mut orient_diff = [0; 3];
            for j in 0..3 {
                let ev = info.edge_values[info.face_edge_ids[i][j] as usize];
                let (a, b) = compat_orientation_extrinsic_index_4(
                    l.q[ev.x as usize],
                    l.n[ev.x as usize],
                    orient.q,
                    orient.n,
                );
                let mut target = (b - a + 4) % 4;
                if face[j] == ev.y {
                    target = (target + 2) % 4;
                }
                orient_diff[j] = (info.face_edge_orients[i][j] - target + 4) % 4;
            }
            if orient_diff[0] == orient_diff[1] {
                orient.orient = orient_diff[0];
            } else if orient_diff[0] == orient_diff[2] {
                orient.orient = orient_diff[2];
            } else if orient_diff[1] == orient_diff[2] {
                orient.orient = orient_diff[1];
            }
            orient.d = orient_diff.map(|d| (d - orient.orient + 4) % 4);
            orient
        })
        .collect()
}

pub fn subdivide_edge_diff(
    p: &mut Parametrizer,
    info: &mut EdgeInfo,
    max_len: i32,
) -> Result<usize, RetopoError> {
    let face_count = p.hierarchy.faces.len();
    let mut diffs = vec![[0, 0]; face_count * 3];
    for i in 0..face_count {
        for j in 0..3 {
            let e = info.face_edge_ids[i][j] as usize;
            diffs[i * 3 + j] = rshift90(info.edge_diff[e], info.face_edge_orients[i][j]);
        }
    }
    let spaces = face_spaces(p, info);
    let mut s = EdgeSplit {
        p,
        info,
        spaces,
        diffs,
        queue: BinaryHeap::new(),
        seq: 0,
    };
    for i in 0..(face_count * 3) as u32 {
        let face = s.p.hierarchy.faces[(i / 3) as usize];
        let (v0, v1) = (face[(i % 3) as usize], face[((i + 1) % 3) as usize]);
        if s.p.non_manifold[v0 as usize] || s.p.non_manifold[v1 as usize] {
            continue;
        }
        let other = s.p.hierarchy.e2e[i as usize];
        if other == INVALID || other > i {
            s.schedule(i, max_len);
        }
    }

    let mut splits = 0;
    while let Some(entry) = s.queue.pop() {
        let e0 = entry.id;
        let e1 = s.p.hierarchy.e2e[e0 as usize];
        let is_boundary = e1 == INVALID;
        let f0 = (e0 / 3) as usize;
        let at = |e: u32, k: u32| ((e + k) % 3) as usize;
        let face0 = s.p.hierarchy.faces[f0];
        let (v0, v0p, v1) = (face0[at(e0, 0)], face0[at(e0, 2)], face0[at(e0, 1)]);
        if squared_length(&s.p.hierarchy.levels[0].v, v0, v1) != entry.length {
            continue;
        }
        let d01 = s.diffs[e0 as usize];
        if d01[0].abs() < 2 && d01[1].abs() < 2 {
            continue;
        }
        let f1 = if is_boundary {
            usize::MAX
        } else {
            (e1 / 3) as usize
        };
        let v1p = if is_boundary {
            INVALID
        } else {
            s.p.hierarchy.faces[f1][at(e1, 2)]
        };

        let vn = s.p.hierarchy.levels[0].v.len() as u32;
        splits += 1;
        {
            let l = &mut s.p.hierarchy.levels[0];
            let (a, b) = (v0 as usize, v1 as usize);
            let (v, n, q, o) = (
                (l.v[a] + l.v[b]) * 0.5,
                l.n[a],
                l.q[a],
                (l.o[a] + l.o[b]) * 0.5,
            );
            l.v.push(v);
            l.n.push(n);
            l.q.push(q);
            l.o.push(o);
        }
        s.p.non_manifold.push(false);
        s.p.boundary.push(is_boundary);
        s.p.v2e.push(INVALID);

        let eid0 = s.info.face_edge_ids[f0][at(e0, 0)];
        let eid01 = s.info.face_edge_ids[f0][at(e0, 1)];
        let eid02 = s.info.face_edge_ids[f0][at(e0, 2)];
        s.info.edge_values[eid0 as usize] = DEdge::new(v0, vn);
        let eid1 = s.info.edge_values.len() as u32;
        s.info.edge_values.push(DEdge::new(vn, v1));
        s.info.edge_diff.push([0, 0]);
        let eid0p = s.info.edge_values.len() as u32;
        s.info.edge_values.push(DEdge::new(vn, v0p));
        s.info.edge_diff.push([0, 0]);

        let f2 = if is_boundary {
            usize::MAX
        } else {
            s.new_face()
        };
        let f3 = s.new_face();

        let d1p = s.diffs[f0 * 3 + at(e0, 1)];
        let dp0 = s.diffs[f0 * 3 + at(e0, 2)];
        let d0n = [d01[0] / 2, d01[1] / 2];

        let orients1 = s.spaces[f0];
        s.p.hierarchy.faces[f0] = [vn, v0p, v0];
        s.info.face_edge_ids[f0] = [eid0p, eid02, eid0];
        s.diffs[f0 * 3] = sub(plus(d01, d1p), d0n);
        s.diffs[f0 * 3 + 1] = dp0;
        s.diffs[f0 * 3 + 2] = d0n;
        let o1 = at(e0, 0);
        s.analyze_orient(f0, [0, orients1.d[(o1 + 2) % 3], orients1.d[o1]]);
        if !is_boundary {
            let o2 = at(e1, 0);
            let orients2 = s.spaces[f1];
            let eid11 = s.info.face_edge_ids[f1][at(e1, 1)];
            let eid12 = s.info.face_edge_ids[f1][at(e1, 2)];
            let ds10 = s.diffs[e1 as usize];
            let ds0p = s.diffs[f1 * 3 + at(e1, 1)];
            let dsp1 = s.diffs[f1 * 3 + at(e1, 2)];
            let orient =
                (0..4)
                    .find(|&o| rshift90(d01, o) == ds10)
                    .ok_or(RetopoError::RouteBInvariant {
                        stage: "edge split met twin offsets that are not rotations of each other",
                    })?;
            let dsn0 = rshift90(d0n, orient);

            s.p.hierarchy.faces[f1] = [vn, v0, v1p];
            let eid1p = s.info.edge_values.len() as u32;
            s.info.edge_values.push(DEdge::new(vn, v1p));
            s.info.edge_diff.push([0, 0]);
            s.info.face_edge_ids[f1] = [eid0, eid11, eid1p];
            s.diffs[f1 * 3] = dsn0;
            s.diffs[f1 * 3 + 1] = ds0p;
            s.diffs[f1 * 3 + 2] = plus(dsp1, sub(ds10, dsn0));
            s.analyze_orient(f1, [orients2.d[o2], orients2.d[(o2 + 1) % 3], 0]);

            s.spaces[f2] = s.spaces[f1];
            s.info.face_edge_ids[f2] = [eid1p, eid12, eid1];
            s.p.hierarchy.faces[f2] = [vn, v1p, v1];
            s.diffs[f2 * 3] = sub(neg(dsp1), sub(ds10, dsn0));
            s.diffs[f2 * 3 + 1] = dsp1;
            s.diffs[f2 * 3 + 2] = sub(ds10, dsn0);
            s.analyze_orient(f2, [0, orients2.d[(o2 + 2) % 3], orients2.d[o2]]);
        }
        s.spaces[f3] = s.spaces[f0];
        s.info.face_edge_ids[f3] = [eid1, eid01, eid0p];
        s.p.hierarchy.faces[f3] = [vn, v1, v0p];
        s.diffs[f3 * 3] = sub(d01, d0n);
        s.diffs[f3 * 3 + 1] = d1p;
        s.diffs[f3 * 3 + 2] = sub(d0n, plus(d01, d1p));
        s.analyze_orient(f3, [orients1.d[o1], orients1.d[(o1 + 1) % 3], 0]);

        s.fix_orient(f0);
        if !is_boundary {
            s.fix_orient(f1);
            s.fix_orient(f2);
        }
        s.fix_orient(f3);

        let (f0, f3) = (f0 as u32, f3 as u32);
        let e2e = &mut s.p.hierarchy.e2e;
        let e0p = e2e[dedge_prev(e0, 3) as usize];
        let e0n = e2e[dedge_next(e0, 3) as usize];
        link(e2e, 3 * f0, 3 * f3 + 2);
        link(e2e, 3 * f0 + 1, e0p);
        link(e2e, 3 * f3 + 1, e0n);
        if is_boundary {
            link(e2e, 3 * f0 + 2, INVALID);
            link(e2e, 3 * f3, INVALID);
        } else {
            let (f1, f2) = (f1 as u32, f2 as u32);
            let e1p = e2e[dedge_prev(e1, 3) as usize];
            let e1n = e2e[dedge_next(e1, 3) as usize];
            link(e2e, 3 * f0 + 2, 3 * f1);
            link(e2e, 3 * f1 + 1, e1n);
            link(e2e, 3 * f1 + 2, 3 * f2);
            link(e2e, 3 * f2 + 1, e1p);
            link(e2e, 3 * f2 + 2, 3 * f3);
        }

        let v2e = &mut s.p.v2e;
        v2e[v0 as usize] = 3 * f0 + 2;
        v2e[vn as usize] = 3 * f0;
        v2e[v1 as usize] = 3 * f3 + 1;
        v2e[v0p as usize] = 3 * f0 + 1;
        if !is_boundary {
            v2e[v1p as usize] = 3 * f1 as u32 + 2;
        }

        let mut order = vec![f0];
        if !is_boundary {
            order.extend([f2 as u32, f1 as u32]);
        }
        order.push(f3);
        for f in order {
            for i in 0..3 {
                s.schedule(f * 3 + i, max_len);
            }
        }
    }

    let too_long = RetopoError::RouteBInvariant {
        stage: "edge split left an edge offset longer than one",
    };
    for ids in &s.info.face_edge_ids {
        if ids
            .iter()
            .any(|&e| exceeds(s.info.edge_diff[e as usize], 1))
        {
            return Err(too_long);
        }
    }
    if s.info.edge_diff.iter().any(|&d| exceeds(d, 1)) {
        return Err(too_long);
    }
    Ok(splits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_b::dedge::DirectedGraph;
    use skp_core::geometry::area_vector;

    fn p(x: f64, y: f64) -> Vec3 {
        Vec3::new(x, y, 0.0)
    }

    fn run(faces: Vec<[u32; 3]>, positions: Vec<Vec3>, rho: Vec<f64>, max: f64) -> Subdivided {
        let g = DirectedGraph::build(positions.len(), &faces);
        subdivide(faces, positions, rho, g.e2e, &g.non_manifold, max)
    }

    fn area(s: &Subdivided) -> f64 {
        s.faces
            .iter()
            .map(|f| {
                let [a, b, c] = f.map(|v| s.positions[v as usize]);
                area_vector(a, b, c).length() * 0.5
            })
            .sum()
    }

    fn assert_twins_consistent(s: &Subdivided) {
        for (e, &t) in s.e2e.iter().enumerate() {
            if t == INVALID {
                continue;
            }
            assert_eq!(s.e2e[t as usize], e as u32);
            let edge = |e: usize| {
                let f = s.faces[e / 3];
                (f[e % 3], f[(e + 1) % 3])
            };
            let (a, b) = edge(e);
            assert_eq!(edge(t as usize), (b, a));
        }
    }

    #[test]
    fn a_long_boundary_edge_is_split_at_its_midpoint_with_upstream_face_order() {
        let s = run(
            vec![[0, 1, 2]],
            vec![p(0.0, 0.0), p(1.0, 0.0), p(0.0, 1.0)],
            vec![1.0, 1.0, 3.0],
            1.2,
        );
        assert_eq!(s.splits, 1);
        assert_eq!(s.faces, [[3, 0, 1], [3, 2, 0]]);
        assert_eq!(s.positions[3], p(0.5, 0.5));
        assert_eq!(s.e2e, [5, INVALID, INVALID, INVALID, INVALID, 0]);
        assert_eq!(s.rho[3], 1.5);
    }

    #[test]
    fn a_long_interior_edge_splits_both_faces() {
        let s = run(
            vec![[0, 1, 2], [0, 2, 3]],
            vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0), p(0.0, 1.0)],
            vec![1.0; 4],
            1.2,
        );
        assert_eq!(s.splits, 1);
        assert_eq!(s.faces.len(), 4);
        assert_eq!(s.positions[4], p(0.5, 0.5));
        assert_twins_consistent(&s);
        assert!((area(&s) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn every_edge_ends_within_the_bound_and_the_surface_is_unchanged() {
        let s = run(
            vec![[0, 1, 2], [0, 2, 3]],
            vec![p(0.0, 0.0), p(8.0, 0.0), p(8.0, 3.0), p(0.0, 3.0)],
            vec![1.0; 4],
            1.0,
        );
        assert!(s.splits > 10);
        assert_twins_consistent(&s);
        assert!((area(&s) - 24.0).abs() < 1e-9);
        for (f, face) in s.faces.iter().enumerate() {
            for i in 0..3 {
                let (a, b) = (face[i], face[(i + 1) % 3]);
                let length = squared_length(&s.positions, a, b);
                assert!(
                    !too_long(length, 1.0, s.rho[a as usize], s.rho[b as usize]),
                    "face {f} edge {i} squared length {length}"
                );
            }
        }
    }

    #[test]
    fn edges_touching_a_non_manifold_vertex_are_not_queued() {
        let s = run(
            vec![[0, 1, 2], [1, 0, 3], [1, 0, 4]],
            vec![
                p(0.0, 0.0),
                p(10.0, 0.0),
                p(5.0, 1.0),
                p(5.0, -1.0),
                Vec3::new(5.0, 0.0, 1.0),
            ],
            vec![100.0; 5],
            1.0,
        );
        assert_eq!(s.splits, 0);
    }

    fn square_with_long_offsets() -> (Parametrizer, EdgeInfo) {
        let faces = vec![[0, 1, 2], [0, 2, 3]];
        let v = vec![p(0.0, 0.0), p(2.0, 0.0), p(2.0, 2.0), p(0.0, 2.0)];
        let g = DirectedGraph::build(4, &faces);
        let hierarchy = crate::route_b::hierarchy::Hierarchy {
            faces,
            e2e: g.e2e,
            scale: 1.0,
            levels: vec![crate::route_b::hierarchy::Level {
                o: v.clone(),
                v,
                n: vec![Vec3::new(0.0, 0.0, 1.0); 4],
                q: vec![Vec3::new(1.0, 0.0, 0.0); 4],
                ..Default::default()
            }],
            ..Default::default()
        };
        let field = Parametrizer {
            v2e: g.v2e,
            boundary: g.boundary,
            non_manifold: g.non_manifold,
            hierarchy,
            ..Parametrizer::default()
        };
        let info = EdgeInfo {
            edge_values: vec![
                DEdge::new(0, 1),
                DEdge::new(1, 2),
                DEdge::new(0, 2),
                DEdge::new(2, 3),
                DEdge::new(0, 3),
            ],
            edge_diff: vec![[2, 0], [0, 2], [2, 2], [-2, 0], [0, 2]],
            face_edge_ids: vec![[0, 1, 2], [2, 3, 4]],
            face_edge_orients: vec![[0, 0, 2], [0, 0, 2]],
        };
        (field, info)
    }

    fn face_closes(info: &EdgeInfo, f: usize) -> bool {
        let sum = (0..3).fold([0, 0], |acc, j| {
            let d = rshift90(
                info.edge_diff[info.face_edge_ids[f][j] as usize],
                info.face_edge_orients[f][j],
            );
            [acc[0] + d[0], acc[1] + d[1]]
        });
        sum == [0, 0]
    }

    #[test]
    fn the_longest_offset_splits_first_in_queue_order() {
        let (mut field, mut info) = square_with_long_offsets();
        assert!((0..2).all(|f| face_closes(&info, f)));
        let splits = subdivide_edge_diff(&mut field, &mut info, 1).unwrap();
        assert!(splits >= 3, "{splits} splits");
        assert_eq!(field.hierarchy.levels[0].v[4], p(1.0, 0.0));
    }

    #[test]
    fn after_the_edge_split_every_offset_is_at_most_one_and_every_face_closes() {
        let (mut field, mut info) = square_with_long_offsets();
        subdivide_edge_diff(&mut field, &mut info, 1).unwrap();
        let h = &field.hierarchy;
        assert!(info
            .edge_diff
            .iter()
            .all(|d| d[0].abs() <= 1 && d[1].abs() <= 1));
        for f in 0..h.faces.len() {
            assert!(face_closes(&info, f), "face {f}");
            for j in 0..3 {
                let (a, b) = (h.faces[f][j], h.faces[f][(j + 1) % 3]);
                assert_eq!(
                    info.edge_values[info.face_edge_ids[f][j] as usize],
                    DEdge::new(a, b)
                );
            }
        }
        let s = Subdivided {
            faces: h.faces.clone(),
            positions: h.levels[0].v.clone(),
            rho: Vec::new(),
            e2e: h.e2e.clone(),
            splits: 0,
        };
        assert_twins_consistent(&s);
        assert!((area(&s) - 4.0).abs() < 1e-12);
        assert_eq!(field.v2e.len(), h.levels[0].v.len());
    }
}
