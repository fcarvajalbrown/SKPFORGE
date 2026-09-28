use super::dedge::INVALID;
use super::extract::QuadMesh;
use std::collections::{BTreeMap, BinaryHeap};

fn prev4(e: u32) -> u32 {
    e / 4 * 4 + (e + 3) % 4
}

fn next4(e: u32) -> u32 {
    e / 4 * 4 + (e + 1) % 4
}

fn corner(m: &QuadMesh, e: u32, k: u32) -> u32 {
    m.faces[(e / 4) as usize][((e + k) % 4) as usize]
}

fn set_corner(m: &mut QuadMesh, e: u32, v: u32) {
    m.faces[(e / 4) as usize][(e % 4) as usize] = v;
}

fn keep_faces(m: &mut QuadMesh, erased: &[bool]) {
    let mut i = 0;
    m.faces.retain(|_| {
        let keep = !erased[i];
        i += 1;
        keep
    });
}

fn remove_valence_two(m: &mut QuadMesh) {
    loop {
        let mut update = false;
        let mut marks = vec![false; m.v2e.len()];
        let mut erased = vec![false; m.faces.len()];
        for i in 0..m.v2e.len() {
            let deid0 = m.v2e[i];
            if marks[i] || deid0 == INVALID {
                continue;
            }
            let mut dedges = Vec::new();
            let mut deid = deid0;
            loop {
                dedges.push(deid);
                deid = m.e2e[prev4(deid) as usize];
                if deid == deid0 || deid == INVALID {
                    break;
                }
            }
            if dedges.len() != 2 {
                continue;
            }
            let (d0, d1) = (dedges[0], dedges[1]);
            let vs = [
                corner(m, d0, 1),
                corner(m, d0, 2),
                corner(m, d1, 1),
                corner(m, d1, 2),
            ];
            if vs.iter().any(|&v| marks[v as usize]) {
                continue;
            }
            for &v in &vs {
                marks[v as usize] = true;
            }
            let repeated = (0..3).any(|a| (a + 1..4).any(|b| vs[a] == vs[b]));
            if repeated {
                erased[(d0 / 4) as usize] = true;
            } else {
                m.faces[(d0 / 4) as usize] = vs;
            }
            erased[(d1 / 4) as usize] = true;
            update = true;
        }
        if !update {
            return;
        }
        keep_faces(m, &erased);
        m.rebuild_graph();
    }
}

fn split_fans(m: &mut QuadMesh) {
    let mut v_dedges: Vec<Vec<u32>> = vec![Vec::new(); m.v2e.len()];
    for (i, f) in m.faces.iter().enumerate() {
        for (j, &v) in f.iter().enumerate() {
            v_dedges[v as usize].push((i * 4 + j) as u32);
        }
    }
    let mut top = m.v2e.len() as u32;
    for (i, dedges) in v_dedges.iter().enumerate() {
        let mut groups: BTreeMap<u32, u32> = BTreeMap::new();
        let mut group_id = 0;
        for &start in dedges {
            if groups.contains_key(&start) {
                continue;
            }
            let mut deid = start;
            loop {
                groups.insert(deid, group_id);
                deid = m.e2e[prev4(deid) as usize];
                if deid == start || deid == INVALID {
                    break;
                }
            }
            if deid == INVALID {
                deid = start;
                while m.e2e[deid as usize] != INVALID {
                    deid = next4(m.e2e[deid as usize]);
                    groups.insert(deid, group_id);
                }
            }
            group_id += 1;
        }
        if group_id > 1 {
            for (&d, &g) in &groups {
                if g >= 1 {
                    set_corner(m, d, top - 1 + g);
                }
            }
            for _ in 1..group_id {
                m.push_copy_of(i);
            }
            top = m.o.len() as u32;
        }
    }
    m.rebuild_graph();
}

fn grown<T: Clone + Default>(v: &mut Vec<T>, i: usize) -> &mut T {
    if i >= v.len() {
        v.resize(i + 1, T::default());
    }
    &mut v[i]
}

fn decrease_valence(m: &mut QuadMesh) {
    loop {
        let mut update = false;
        let mut marks = vec![false; m.v2e.len()];
        let mut valences = vec![0i32; m.v2e.len()];
        for (i, valence) in valences.iter_mut().enumerate() {
            let deid0 = m.v2e[i];
            if deid0 == INVALID {
                continue;
            }
            let mut deid = deid0;
            let mut count = 0;
            loop {
                count += 1;
                let deid1 = m.e2e[deid as usize];
                if deid1 == INVALID {
                    count += 1;
                    break;
                }
                deid = next4(deid1);
                if deid == deid0 {
                    break;
                }
            }
            *valence = count;
        }
        let mut queue: BinaryHeap<(i32, u32)> = valences
            .iter()
            .enumerate()
            .filter(|(_, &v)| v > 5)
            .map(|(i, &v)| (v, i as u32))
            .collect();
        while let Some((valence, v)) = queue.pop() {
            if *grown(&mut marks, v as usize) {
                continue;
            }
            let deid0 = m.v2e[v as usize];
            if deid0 == INVALID {
                continue;
            }
            let mut loop_vertices = Vec::new();
            let mut loop_dedges = Vec::new();
            let mut marked = false;
            let mut deid = deid0;
            loop {
                let u = corner(m, deid, 1);
                loop_dedges.push(deid);
                loop_vertices.push(u);
                if *grown(&mut marks, u as usize) {
                    marked = true;
                }
                let deid1 = m.e2e[deid as usize];
                if deid1 == INVALID {
                    break;
                }
                deid = next4(deid1);
                if deid == deid0 {
                    break;
                }
            }
            if marked {
                continue;
            }
            let len = loop_vertices.len();
            let step = ((valence + 1) / 2) as usize;
            let mut min_val = (i32::MAX, i32::MAX);
            let mut split_idx = usize::MAX;
            for i in 0..len {
                if i + step >= len {
                    continue;
                }
                let mut v1 = *grown(&mut valences, loop_vertices[i] as usize);
                let mut v2 = *grown(&mut valences, loop_vertices[i + step] as usize);
                if v1 < v2 {
                    std::mem::swap(&mut v1, &mut v2);
                }
                if (v1, v2) < min_val {
                    min_val = (v1, v2);
                    split_idx = i + 1;
                }
            }
            if min_val.0 >= valence {
                continue;
            }
            update = true;
            let new_vertex = m.o.len() as u32;
            for &d in &loop_dedges[split_idx..split_idx + step] {
                set_corner(m, d, new_vertex);
            }
            m.faces.push([
                new_vertex,
                loop_vertices[(split_idx + len - 1) % len],
                v,
                loop_vertices[(split_idx + step - 1 + len) % len],
            ]);
            *grown(&mut marks, v as usize) = true;
            for &u in &loop_vertices {
                *grown(&mut marks, u as usize) = true;
            }
            m.push_copy_of(v as usize);
        }
        if !update {
            return;
        }
        m.rebuild_graph();
    }
}

fn remove_unused(m: &mut QuadMesh) {
    let mut used = vec![false; m.v2e.len().max(m.o.len())];
    for f in &m.faces {
        for &v in f {
            used[v as usize] = true;
        }
    }
    let mut compact = vec![0u32; used.len()];
    let mut top = 0;
    for i in 0..m.o.len() {
        if !used[i] {
            continue;
        }
        m.n[top] = m.n[i];
        m.o[top] = m.o[i];
        m.q[top] = m.q[i];
        m.vset.swap(top, i);
        compact[i] = top as u32;
        top += 1;
    }
    for f in &mut m.faces {
        for v in f.iter_mut() {
            *v = compact[*v as usize];
        }
    }
    m.n.truncate(top);
    m.o.truncate(top);
    m.q.truncate(top);
    m.vset.truncate(top);
    m.rebuild_graph();
}

pub fn fix_valence(m: &mut QuadMesh) {
    remove_valence_two(m);
    split_fans(m);
    decrease_valence(m);
    remove_unused(m);
}

#[cfg(test)]
mod tests {
    use super::*;
    use skp_core::geometry::Vec3;

    fn mesh_with(points: usize, faces: Vec<[u32; 4]>) -> QuadMesh {
        let mut m = QuadMesh {
            o: (0..points).map(|i| Vec3::new(i as f64, 0.0, 0.0)).collect(),
            n: vec![Vec3::new(0.0, 0.0, 1.0); points],
            q: vec![Vec3::new(1.0, 0.0, 0.0); points],
            vset: (0..points).map(|i| vec![i as u32]).collect(),
            faces,
            ..QuadMesh::default()
        };
        m.rebuild_graph();
        m
    }

    #[test]
    fn a_vertex_held_by_two_quads_is_removed_and_they_merge() {
        let mut m = mesh_with(5, vec![[0, 1, 2, 3], [0, 3, 4, 1]]);
        fix_valence(&mut m);
        assert_eq!(m.faces, [[0, 1, 2, 3]]);
        assert_eq!(m.vset, [vec![1], vec![2], vec![3], vec![4]]);
    }

    #[test]
    fn a_vertex_used_by_no_quad_is_dropped() {
        let mut m = mesh_with(6, vec![[1, 2, 3, 4], [2, 5, 3, 1]]);
        remove_unused(&mut m);
        assert_eq!(m.o.len(), 5);
        assert!(m.faces.iter().flatten().all(|&v| v < 5));
    }

    #[test]
    fn a_cube_stays_closed_and_sphere_like_after_valence_fixing() {
        use crate::route_b::extract::tests::extracted;
        use crate::route_b::orient::tests::cube;
        let mut m = extracted(cube(), 300);
        fix_valence(&mut m);
        let open = m.e2e.iter().filter(|&&e| e == INVALID).count();
        let (v, f) = (m.o.len() as i64, m.faces.len() as i64);
        assert_eq!(open, 0);
        assert_eq!(v - f, 2);
        let used: std::collections::BTreeSet<u32> = m.faces.iter().flatten().copied().collect();
        assert_eq!(used.len(), m.o.len());
    }

    #[test]
    fn a_flat_patch_keeps_every_vertex_in_use_after_valence_fixing() {
        use crate::route_b::extract::tests::extracted;
        use crate::route_b::orient::tests::flat;
        let mut m = extracted(flat(4), 200);
        let before = m.faces.len();
        fix_valence(&mut m);
        assert_eq!(m.faces.len(), before);
        let used: std::collections::BTreeSet<u32> = m.faces.iter().flatten().copied().collect();
        assert_eq!(used.len(), m.o.len());
    }
}
