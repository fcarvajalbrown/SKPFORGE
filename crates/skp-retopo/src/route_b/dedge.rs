pub const INVALID: u32 = u32::MAX;

pub fn dedge_prev(e: u32, deg: u32) -> u32 {
    if e.is_multiple_of(deg) {
        e + (deg - 1)
    } else {
        e - 1
    }
}

pub fn dedge_next(e: u32, deg: u32) -> u32 {
    if e % deg == deg - 1 {
        e + 1 - deg
    } else {
        e + 1
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectedGraph {
    pub v2e: Vec<u32>,
    pub e2e: Vec<u32>,
    pub boundary: Vec<bool>,
    pub non_manifold: Vec<bool>,
}

impl DirectedGraph {
    pub fn build<const D: usize>(vertex_count: usize, faces: &[[u32; D]]) -> DirectedGraph {
        let deg = D as u32;
        let mut v2e = vec![INVALID; vertex_count];
        let mut tmp = vec![(0u32, INVALID); faces.len() * D];
        for (f, face) in faces.iter().enumerate() {
            for i in 0..D {
                let (cur, next) = (face[i], face[(i + 1) % D]);
                if cur == next {
                    continue;
                }
                let edge_id = deg * f as u32 + i as u32;
                tmp[edge_id as usize] = (next, INVALID);
                if v2e[cur as usize] == INVALID {
                    v2e[cur as usize] = edge_id;
                } else {
                    let mut idx = v2e[cur as usize];
                    while tmp[idx as usize].1 != INVALID {
                        idx = tmp[idx as usize].1;
                    }
                    tmp[idx as usize].1 = edge_id;
                }
            }
        }

        let mut non_manifold = vec![false; vertex_count];
        let mut e2e = vec![INVALID; faces.len() * D];
        for (f, face) in faces.iter().enumerate() {
            for i in 0..D {
                let (cur, next) = (face[i], face[(i + 1) % D]);
                if cur == next {
                    continue;
                }
                let edge_cur = deg * f as u32 + i as u32;
                let mut it = v2e[next as usize];
                let mut edge_opp = INVALID;
                while it != INVALID {
                    if tmp[it as usize].0 == cur {
                        if edge_opp == INVALID {
                            edge_opp = it;
                        } else {
                            non_manifold[cur as usize] = true;
                            non_manifold[next as usize] = true;
                            edge_opp = INVALID;
                            break;
                        }
                    }
                    it = tmp[it as usize].1;
                }
                let mut same = 0;
                let mut it = v2e[cur as usize];
                while it != INVALID {
                    if tmp[it as usize].0 == next {
                        same += 1;
                    }
                    it = tmp[it as usize].1;
                }
                if edge_opp != INVALID && same == 1 && edge_cur < edge_opp {
                    e2e[edge_cur as usize] = edge_opp;
                    e2e[edge_opp as usize] = edge_cur;
                }
            }
        }

        let mut boundary = vec![false; vertex_count];
        for i in 0..vertex_count {
            let mut edge = v2e[i];
            if edge == INVALID {
                continue;
            }
            if non_manifold[i] {
                v2e[i] = INVALID;
                continue;
            }
            let start = edge;
            let mut first = INVALID;
            loop {
                first = first.min(edge);
                let prev = e2e[dedge_prev(edge, deg) as usize];
                if prev == INVALID {
                    first = edge;
                    boundary[i] = true;
                    break;
                }
                edge = prev;
                if edge == start {
                    break;
                }
            }
            v2e[i] = first;
        }

        DirectedGraph {
            v2e,
            e2e,
            boundary,
            non_manifold,
        }
    }
}

pub fn split_non_manifold(faces: &mut [[u32; 3]], e2e: &[u32], vertex_count: usize) -> Vec<u32> {
    let mut vert_to_edges: Vec<Vec<u32>> = vec![Vec::new(); vertex_count];
    for (i, f) in faces.iter().enumerate() {
        for (j, &v) in f.iter().enumerate() {
            vert_to_edges[v as usize].push((i * 3 + j) as u32);
        }
    }
    let mut coloured = vec![false; faces.len() * 3];
    let mut copies = Vec::new();
    let mut next_vertex = vertex_count as u32;
    for (i, edges) in vert_to_edges.iter().enumerate() {
        let mut colours = 0;
        for &start in edges {
            if coloured[start as usize] {
                continue;
            }
            let mut fan = vec![start];
            let mut deid = start;
            loop {
                deid = e2e[dedge_prev(deid, 3) as usize];
                if deid == INVALID || deid == start {
                    break;
                }
                fan.push(deid);
            }
            if deid == INVALID {
                deid = start;
                loop {
                    let twin = e2e[deid as usize];
                    if twin == INVALID {
                        break;
                    }
                    deid = dedge_next(twin, 3);
                    if deid == start {
                        break;
                    }
                    fan.push(deid);
                }
            }
            for &d in &fan {
                coloured[d as usize] = true;
                if colours != 0 {
                    faces[(d / 3) as usize][(d % 3) as usize] = next_vertex;
                }
            }
            if colours != 0 {
                copies.push(i as u32);
                next_vertex += 1;
            }
            colours += 1;
        }
    }
    copies
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prev_and_next_wrap_inside_their_face() {
        assert_eq!(dedge_prev(3, 3), 5);
        assert_eq!(dedge_prev(5, 3), 4);
        assert_eq!(dedge_next(5, 3), 3);
        assert_eq!(dedge_next(4, 4), 5);
        assert_eq!(dedge_next(7, 4), 4);
    }

    #[test]
    fn two_triangles_pair_their_shared_edge_and_start_each_fan_at_a_boundary() {
        let g = DirectedGraph::build(4, &[[0, 1, 2], [0, 2, 3]]);
        assert_eq!(g.e2e, [INVALID, INVALID, 3, 2, INVALID, INVALID]);
        assert_eq!(g.v2e, [3, 1, 2, 5]);
        assert_eq!(g.boundary, [true; 4]);
        assert_eq!(g.non_manifold, [false; 4]);
    }

    #[test]
    fn a_closed_fan_starts_at_its_smallest_outgoing_edge() {
        let faces = [[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let g = DirectedGraph::build(4, &faces);
        assert!(g.e2e.iter().all(|&e| e != INVALID));
        assert_eq!(g.boundary, [false; 4]);
        assert_eq!(g.v2e, [0, 2, 1, 5]);
    }

    #[test]
    fn a_third_face_on_an_edge_marks_both_ends_non_manifold_and_drops_their_fans() {
        let g = DirectedGraph::build(5, &[[0, 1, 2], [1, 0, 3], [1, 0, 4]]);
        assert_eq!(g.non_manifold, [true, true, false, false, false]);
        assert_eq!(g.v2e[0], INVALID);
        assert_eq!(g.v2e[1], INVALID);
        assert_eq!(g.e2e[0], INVALID);
    }

    #[test]
    fn an_edge_whose_direction_repeats_is_left_unpaired_so_twins_stay_mutual() {
        let g = DirectedGraph::build(5, &[[0, 1, 2], [1, 0, 3], [0, 1, 4]]);
        assert_eq!(g.e2e[0], INVALID);
        assert_eq!(g.e2e[3], INVALID);
        assert_eq!(g.e2e[6], INVALID);
        for (e, &t) in g.e2e.iter().enumerate() {
            assert!(t == INVALID || g.e2e[t as usize] == e as u32);
        }
    }

    #[test]
    fn faces_on_an_unpaired_edge_each_get_their_own_copies_of_its_ends() {
        let mut faces = [[0, 1, 2], [1, 0, 3], [0, 1, 4]];
        let g = DirectedGraph::build(5, &faces);
        let copies = split_non_manifold(&mut faces, &g.e2e, 5);
        assert_eq!(copies, [0, 0, 1, 1]);
        let g = DirectedGraph::build(9, &faces);
        assert!(g.non_manifold.iter().all(|&b| !b));
        assert!(split_non_manifold(&mut faces, &g.e2e, 9).is_empty());
    }

    #[test]
    fn a_bowtie_vertex_is_split_into_one_copy_per_fan() {
        let mut faces = [[0, 1, 2], [0, 3, 4]];
        let g = DirectedGraph::build(5, &faces);
        let copies = split_non_manifold(&mut faces, &g.e2e, 5);
        assert_eq!(copies, [0]);
        assert_eq!(faces[1][0], 5);
    }

    #[test]
    fn two_quads_pair_their_shared_edge() {
        let g = DirectedGraph::build(6, &[[0, 1, 4, 3], [1, 2, 5, 4]]);
        assert_eq!(g.e2e[1], 7);
        assert_eq!(g.e2e[7], 1);
        assert_eq!(g.e2e.iter().filter(|&&e| e != INVALID).count(), 2);
        assert_eq!(g.boundary, [true; 6]);
    }

    #[test]
    fn a_collapsed_edge_is_skipped() {
        let g = DirectedGraph::build(5, &[[0, 1, 2], [3, 3, 4]]);
        assert_eq!(g.e2e[3], INVALID);
        assert_eq!(g.e2e[4], 5);
        assert_eq!(g.v2e[3], 4);
    }
}
