use super::field_math::rshift90;
use super::integer::EdgeInfo;
use crate::error::RetopoError;
use std::collections::{HashMap, VecDeque};

const MAX_EDGE_LEVELS: usize = 100;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeLevel {
    pub fq: Vec<[i32; 3]>,
    pub f2e: Vec<[u32; 3]>,
    pub e2f: Vec<[i32; 2]>,
    pub edge_diff: Vec<[i32; 2]>,
    pub sing: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EdgeHierarchy {
    pub levels: Vec<EdgeLevel>,
    pub to_upper_edges: Vec<Vec<i32>>,
    pub to_upper_orients: Vec<Vec<i32>>,
    pub to_upper_faces: Vec<Vec<i32>>,
}

fn face_sum(level: &EdgeLevel, f: usize) -> [i32; 2] {
    (0..3).fold([0, 0], |acc, j| {
        let d = rshift90(level.edge_diff[level.f2e[f][j] as usize], level.fq[f][j]);
        [acc[0] + d[0], acc[1] + d[1]]
    })
}

fn area(level: &EdgeLevel, f: usize) -> i32 {
    let d1 = rshift90(level.edge_diff[level.f2e[f][0] as usize], level.fq[f][0]);
    let d2 = rshift90(level.edge_diff[level.f2e[f][1] as usize], level.fq[f][1]);
    d1[0] * d2[1] - d1[1] * d2[0]
}

struct Downsampled {
    level: EdgeLevel,
    to_upper: Vec<i32>,
    to_upper_orients: Vec<i32>,
    upper_face: Vec<i32>,
}

fn downsample(level: &EdgeLevel) -> Result<Downsampled, RetopoError> {
    let (fq, e2f, f2e, edge_diff) = (&level.fq, &level.e2f, &level.f2e, &level.edge_diff);
    let mut fixed = vec![0u8; f2e.len()];
    for &s in &level.sing {
        fixed[s as usize] = 1;
    }
    let mut to_upper = vec![-1i32; e2f.len()];
    let mut to_upper_orients = vec![0i32; e2f.len()];
    let face = |f: i32| f as usize;

    for i in 0..e2f.len() {
        if edge_diff[i] != [0, 0] {
            continue;
        }
        let [a, b] = e2f[i];
        if (a >= 0 && fixed[face(a)] != 0) || (b >= 0 && fixed[face(b)] != 0) {
            continue;
        }
        for f in [a, b] {
            if f < 0 {
                continue;
            }
            for &neighbour_e in &f2e[face(f)] {
                for nf in e2f[neighbour_e as usize] {
                    if nf >= 0 && fixed[face(nf)] == 0 {
                        fixed[face(nf)] = 1;
                    }
                }
            }
        }
        for f in [a, b] {
            if f >= 0 {
                fixed[face(f)] = 2;
            }
        }
        to_upper[i] = -2;
    }
    let gone = |f: i32, fixed: &[u8]| f < 0 || fixed[face(f)] == 2;
    let kept = |f: i32, fixed: &[u8]| f < 0 || fixed[face(f)] < 2;
    for i in 0..e2f.len() {
        if to_upper[i] == -2 {
            continue;
        }
        if gone(e2f[i][0], &fixed) && gone(e2f[i][1], &fixed) {
            to_upper[i] = -3;
        }
    }

    let mut next = EdgeLevel::default();
    let mut num_e = 0i32;
    for i in 0..to_upper.len() {
        if to_upper[i] != -1 {
            continue;
        }
        if kept(e2f[i][0], &fixed) && kept(e2f[i][1], &fixed) {
            next.e2f.push(e2f[i]);
            to_upper_orients[i] = 0;
            to_upper[i] = num_e;
            num_e += 1;
            continue;
        }
        let f0 = if e2f[i][1] < 0 || fixed[face(e2f[i][0])] < 2 {
            e2f[i][0]
        } else {
            e2f[i][1]
        };
        let mut e = i;
        let mut f = f0;
        let mut paths: Vec<(usize, i32)> = vec![(i, 0)];
        loop {
            if e2f[e][0] == f {
                f = e2f[e][1];
            } else if e2f[e][1] == f {
                f = e2f[e][0];
            }
            if kept(f, &fixed) {
                for j in 0..paths.len() {
                    let (edge, mut orient) = paths[j];
                    to_upper[edge] = num_e;
                    if j > 0 {
                        orient = (orient + to_upper_orients[paths[j - 1].0]) % 4;
                    }
                    to_upper_orients[edge] = orient;
                }
                next.e2f.push([f0, f]);
                num_e += 1;
                break;
            }
            let row = f2e[face(f)];
            let ind0 =
                row.iter()
                    .position(|&x| x as usize == e)
                    .ok_or(RetopoError::RouteBInvariant {
                        stage: "edge hierarchy walked to a face that does not hold the edge",
                    })?;
            let ind1 = row
                .iter()
                .position(|&x| x as usize != e && to_upper[x as usize] != -2);
            match ind1 {
                Some(j1) => {
                    e = row[j1] as usize;
                    paths.push((e, (fq[face(f)][j1] - fq[face(f)][ind0] + 6) % 4));
                }
                None => {
                    if edge_diff[e] != [0, 0] {
                        return Err(RetopoError::RouteBInvariant {
                            stage: "edge hierarchy collapsed an edge with a nonzero offset",
                        });
                    }
                    for &(edge, _) in &paths {
                        to_upper[edge] = num_e;
                        to_upper_orients[edge] = 0;
                    }
                    num_e += 1;
                    next.e2f.push([f0, f0]);
                    break;
                }
            }
        }
    }

    next.edge_diff = vec![[0, 0]; num_e as usize];
    for i in 0..to_upper.len() {
        if to_upper[i] >= 0 && to_upper_orients[i] == 0 {
            next.edge_diff[to_upper[i] as usize] = edge_diff[i];
        }
    }
    let mut upper_face = vec![-1i32; f2e.len()];
    for i in 0..f2e.len() {
        let eid = f2e[i].map(|e| to_upper[e as usize]);
        if eid.iter().all(|&e| e >= 0) {
            let orient: [i32; 3] =
                std::array::from_fn(|j| (fq[i][j] + 4 - to_upper_orients[f2e[i][j] as usize]) % 4);
            upper_face[i] = next.f2e.len() as i32;
            next.f2e.push(eid.map(|e| e as u32));
            next.fq.push(orient);
        }
    }
    for pair in &mut next.e2f {
        for f in pair.iter_mut() {
            if *f >= 0 {
                *f = upper_face[face(*f)];
            }
        }
    }
    for &s in &level.sing {
        if upper_face[s as usize] >= 0 {
            next.sing.push(upper_face[s as usize] as u32);
        }
    }
    Ok(Downsampled {
        level: next,
        to_upper,
        to_upper_orients,
        upper_face,
    })
}

impl EdgeHierarchy {
    pub fn build(
        fq: Vec<[i32; 3]>,
        f2e: Vec<[u32; 3]>,
        edge_diff: Vec<[i32; 2]>,
    ) -> Result<EdgeHierarchy, RetopoError> {
        let mut e2f = vec![[-1i32, -1i32]; edge_diff.len()];
        for (i, ids) in f2e.iter().enumerate() {
            for &e in ids {
                let slot = &mut e2f[e as usize];
                if slot[0] == -1 {
                    slot[0] = i as i32;
                } else {
                    slot[1] = i as i32;
                }
            }
        }
        let mut finest = EdgeLevel {
            fq,
            f2e,
            e2f,
            edge_diff,
            sing: Vec::new(),
        };
        finest.sing = (0..finest.f2e.len())
            .filter(|&f| face_sum(&finest, f) != [0, 0])
            .map(|f| f as u32)
            .collect();
        let mut h = EdgeHierarchy {
            levels: vec![finest],
            ..EdgeHierarchy::default()
        };
        for _ in 0..MAX_EDGE_LEVELS - 1 {
            let current = h.levels.last().expect("finest level");
            let d = downsample(current)?;
            if d.level.edge_diff.len() == current.edge_diff.len() {
                break;
            }
            h.levels.push(d.level);
            h.to_upper_edges.push(d.to_upper);
            h.to_upper_orients.push(d.to_upper_orients);
            h.to_upper_faces.push(d.upper_face);
        }
        Ok(h)
    }

    fn propagate_edge(&mut self) {
        for level in (1..=self.to_upper_edges.len()).rev() {
            let (lower, upper) = self.levels.split_at_mut(level);
            let (lower, upper) = (&mut lower[level - 1], &upper[0]);
            let to_upper = &self.to_upper_edges[level - 1];
            let orients = &self.to_upper_orients[level - 1];
            for (i, &u) in to_upper.iter().enumerate() {
                lower.edge_diff[i] = if u >= 0 {
                    rshift90(upper.edge_diff[u as usize], (4 - orients[i]) % 4)
                } else {
                    [0, 0]
                };
            }
            for (i, &uf) in self.to_upper_faces[level - 1].iter().enumerate() {
                if uf == -1 {
                    continue;
                }
                let eid_orient = upper.fq[uf as usize];
                for j in 0..3 {
                    lower.fq[i][j] = (eid_orient[j] + orients[lower.f2e[i][j] as usize]) % 4;
                }
            }
        }
    }

    pub fn fix_flip(self) -> Result<EdgeHierarchy, RetopoError> {
        let mut stack: Vec<EdgeHierarchy> = Vec::new();
        let mut current = self;
        while shrink_flipped(current.levels.last_mut().expect("a level")) {
            let top = current.levels.last_mut().expect("a level");
            let child = EdgeHierarchy::build(
                std::mem::take(&mut top.fq),
                std::mem::take(&mut top.f2e),
                std::mem::take(&mut top.edge_diff),
            )?;
            stack.push(current);
            current = child;
        }
        current.propagate_edge();
        while let Some(mut parent) = stack.pop() {
            let finest = current.levels.swap_remove(0);
            let top = parent.levels.last_mut().expect("a level");
            top.fq = finest.fq;
            top.f2e = finest.f2e;
            top.edge_diff = finest.edge_diff;
            parent.propagate_edge();
            current = parent;
        }
        Ok(current)
    }

    pub fn into_edge_info(mut self, info: &mut EdgeInfo) {
        let finest = self.levels.swap_remove(0);
        info.face_edge_orients = finest.fq;
        info.face_edge_ids = finest.f2e;
        info.edge_diff = finest.edge_diff;
    }
}

pub(crate) fn level_e2e(level: &EdgeLevel) -> Vec<i32> {
    let mut e2e = vec![-1i32; level.f2e.len() * 3];
    for (i, &[v1, v2]) in level.e2f.iter().enumerate() {
        let find = |f: i32, from_end: bool| -> i32 {
            let row = level.f2e[f as usize];
            let hit = if from_end {
                (0..3).rev().find(|&j| row[j] as usize == i)
            } else {
                (0..3).find(|&j| row[j] as usize == i)
            };
            f * 3 + hit.expect("edge listed in its face") as i32
        };
        if v1 != -1 {
            e2e[find(v1, false) as usize] = if v2 == -1 { -1 } else { find(v2, true) };
        }
        if v2 != -1 {
            e2e[find(v2, true) as usize] = if v1 == -1 { -1 } else { find(v1, false) };
        }
    }
    e2e
}

fn check_shrink(level: &mut EdgeLevel, e2e: &[i32], deid: i32, allowed: i32) -> bool {
    if deid == -1 {
        return false;
    }
    let prev = |d: i32| d / 3 * 3 + (d + 2) % 3;
    let next = |d: i32| d / 3 * 3 + (d + 1) % 3;
    let deid0 = deid;
    let mut deid = deid;
    loop {
        deid = prev(deid);
        if e2e[deid as usize] == -1 {
            break;
        }
        deid = e2e[deid as usize];
        if deid == deid0 {
            break;
        }
    }
    let at = |d: i32| (d / 3) as usize;
    let slot = |d: i32| (d % 3) as usize;
    let mut diff = level.edge_diff[level.f2e[at(deid)][slot(deid)] as usize];
    let mut diffs: Vec<[i32; 2]> = Vec::new();
    let mut edges: Vec<i32> = Vec::new();
    let mut faces: Vec<usize> = Vec::new();
    loop {
        diffs.push(diff);
        edges.push(deid);
        faces.push(at(deid));
        deid = e2e[deid as usize];
        if deid == -1 {
            return false;
        }
        let d = rshift90(diff, level.fq[at(deid)][slot(deid)]);
        diff = [-d[0], -d[1]];
        deid = next(deid);
        diff = rshift90(diff, (4 - level.fq[at(deid)][slot(deid)]) % 4);
        if deid == edges[0] {
            break;
        }
    }
    if diff != diffs[0] {
        return false;
    }
    let mut new_values: HashMap<u32, [i32; 2]> = HashMap::new();
    for &d in &edges {
        let eid = level.f2e[at(d)][slot(d)];
        new_values.insert(eid, level.edge_diff[eid as usize]);
    }
    for (i, &d) in edges.iter().enumerate() {
        let eid = level.f2e[at(d)][slot(d)];
        let res = new_values.get_mut(&eid).expect("value recorded above");
        res[0] -= diffs[i][0];
        res[1] -= diffs[i][1];
        if res[0].abs() > allowed || res[1].abs() > allowed {
            return false;
        }
        if (res[0].abs() > 1 && res[1] != 0) || (res[1].abs() > 1 && res[0] != 0) {
            return false;
        }
    }
    let flipped = |level: &EdgeLevel| faces.iter().filter(|&&f| area(level, f) < 0).count();
    let before = flipped(level);
    for (eid, value) in new_values.iter_mut() {
        std::mem::swap(&mut level.edge_diff[*eid as usize], value);
    }
    if flipped(level) < before {
        return true;
    }
    for (eid, value) in new_values.iter_mut() {
        std::mem::swap(&mut level.edge_diff[*eid as usize], value);
    }
    false
}

fn shrink_flipped(level: &mut EdgeLevel) -> bool {
    let e2e = level_e2e(level);
    let mut flipped: VecDeque<usize> = (0..level.f2e.len())
        .filter(|&f| area(level, f) < 0)
        .collect();
    let mut update = false;
    let mut max_len = 1;
    while !update && max_len <= 2 {
        while let Some(&f) = flipped.front() {
            if area(level, f) >= 0 {
                flipped.pop_front();
                continue;
            }
            for i in 0..3 {
                let d = (f * 3 + i) as i32;
                if check_shrink(level, &e2e, d, max_len)
                    || check_shrink(level, &e2e, e2e[d as usize], max_len)
                {
                    update = true;
                    break;
                }
            }
            flipped.pop_front();
        }
        max_len += 1;
    }
    update
}

pub fn fix_flip_hierarchy(info: &mut EdgeInfo) -> Result<(), RetopoError> {
    let h = EdgeHierarchy::build(
        std::mem::take(&mut info.face_edge_orients),
        std::mem::take(&mut info.face_edge_ids),
        std::mem::take(&mut info.edge_diff),
    )?;
    h.fix_flip()?.into_edge_info(info);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_b::integer::face_sum as info_face_sum;
    use crate::route_b::integer::tests::through_max_flow;
    use crate::route_b::orient::tests::{cube, flat};
    use crate::route_b::subdivide::subdivide_edge_diff;

    fn after_edge_split(
        mesh: (Vec<skp_core::geometry::Vec3>, Vec<[u32; 3]>),
        faces: usize,
    ) -> EdgeInfo {
        let (mut p, mut info, report) = through_max_flow(mesh, faces);
        assert!(report.full);
        subdivide_edge_diff(&mut p, &mut info, 1).unwrap();
        info
    }

    fn hierarchy_of(info: &EdgeInfo) -> EdgeHierarchy {
        EdgeHierarchy::build(
            info.face_edge_orients.clone(),
            info.face_edge_ids.clone(),
            info.edge_diff.clone(),
        )
        .unwrap()
    }

    fn flipped(info: &EdgeInfo) -> usize {
        let level = EdgeLevel {
            fq: info.face_edge_orients.clone(),
            f2e: info.face_edge_ids.clone(),
            edge_diff: info.edge_diff.clone(),
            ..EdgeLevel::default()
        };
        (0..level.f2e.len())
            .filter(|&f| area(&level, f) < 0)
            .count()
    }

    #[test]
    fn every_coarser_edge_level_is_smaller_and_its_faces_still_close() {
        let info = after_edge_split(cube(), 300);
        let h = hierarchy_of(&info);
        assert!(h.levels.len() > 1);
        for pair in h.levels.windows(2) {
            assert!(pair[1].edge_diff.len() < pair[0].edge_diff.len());
        }
        for level in &h.levels {
            let open = (0..level.f2e.len())
                .filter(|&f| face_sum(level, f) != [0, 0])
                .count();
            assert_eq!(open, level.sing.len());
        }
        assert_eq!(h.to_upper_edges.len(), h.levels.len() - 1);
    }

    #[test]
    fn propagating_an_untouched_hierarchy_gives_back_the_finest_offsets() {
        let info = after_edge_split(flat(4), 200);
        let mut h = hierarchy_of(&info);
        h.propagate_edge();
        assert_eq!(h.levels[0].edge_diff, info.edge_diff);
    }

    #[test]
    fn fixing_flips_never_adds_a_flipped_face_and_keeps_every_face_closed() {
        for (mesh, faces) in [(flat(4), 200), (cube(), 300)] {
            let mut info = after_edge_split(mesh, faces);
            let before = flipped(&info);
            fix_flip_hierarchy(&mut info).unwrap();
            assert!(flipped(&info) <= before);
            for f in 0..info.face_edge_ids.len() {
                assert_eq!(info_face_sum(&info, f), [0, 0], "face {f}");
            }
        }
    }

    fn fan_with_one_flipped_face() -> EdgeInfo {
        EdgeInfo {
            edge_diff: vec![
                [-2, 0],
                [1, 0],
                [-1, 0],
                [0, 1],
                [-1, 1],
                [-1, 0],
                [-2, 1],
                [0, 1],
            ],
            face_edge_ids: vec![[0, 1, 2], [2, 3, 4], [4, 5, 6], [6, 7, 0]],
            face_edge_orients: vec![[0, 0, 2], [0, 0, 2], [0, 0, 2], [0, 2, 2]],
            ..EdgeInfo::default()
        }
    }

    #[test]
    fn shrinking_an_edge_unflips_the_fan() {
        let info = fan_with_one_flipped_face();
        assert_eq!(flipped(&info), 1);
        let mut level = hierarchy_of(&info).levels.swap_remove(0);
        assert!(shrink_flipped(&mut level));
        assert_eq!(level.edge_diff[2], [0, 0]);
        assert_eq!(level.edge_diff[0], [-1, 0]);
        assert_eq!(level.edge_diff[6], [-1, 1]);
        assert!((0..4).all(|f| area(&level, f) >= 0 && face_sum(&level, f) == [0, 0]));
    }

    #[test]
    fn the_flip_fix_reaches_the_finest_level_through_the_hierarchy() {
        let mut info = fan_with_one_flipped_face();
        fix_flip_hierarchy(&mut info).unwrap();
        assert_eq!(flipped(&info), 0);
        for f in 0..4 {
            assert_eq!(info_face_sum(&info, f), [0, 0], "face {f}");
        }
    }
}
