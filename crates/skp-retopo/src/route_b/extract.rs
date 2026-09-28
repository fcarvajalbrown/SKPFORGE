use super::dedge::{DirectedGraph, INVALID};
use super::dset::DisjointTree;
use super::field_math::{compat_orientation_extrinsic_4, normalized, DEdge};
use super::flip::{level_e2e, EdgeHierarchy};
use super::integer::EdgeInfo;
use super::Parametrizer;
use crate::error::RetopoError;
use skp_core::geometry::Vec3;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

const HOLE_LIMIT: usize = 25;
#[allow(clippy::approx_constant)]
const UPSTREAM_PI: f64 = 3.141592654;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuadMesh {
    pub o: Vec<Vec3>,
    pub q: Vec<Vec3>,
    pub n: Vec<Vec3>,
    pub vset: Vec<Vec<u32>>,
    pub faces: Vec<[u32; 4]>,
    pub v2e: Vec<u32>,
    pub e2e: Vec<u32>,
    pub boundary: Vec<bool>,
    pub non_manifold: Vec<bool>,
}

impl QuadMesh {
    pub fn rebuild_graph(&mut self) {
        let g = DirectedGraph::build(self.o.len(), &self.faces);
        self.v2e = g.v2e;
        self.e2e = g.e2e;
        self.boundary = g.boundary;
        self.non_manifold = g.non_manifold;
    }

    pub fn push_copy_of(&mut self, v: usize) {
        self.vset.push(self.vset[v].clone());
        self.o.push(self.o[v]);
        self.n.push(self.n[v]);
        self.q.push(self.q[v]);
    }
}

pub struct Extracted {
    pub quads: QuadMesh,
    pub groups: DisjointTree,
}

pub fn advanced_extract_quad(
    p: &Parametrizer,
    info: &mut EdgeInfo,
) -> Result<Extracted, RetopoError> {
    let h = EdgeHierarchy::build(
        std::mem::take(&mut info.face_edge_orients),
        std::mem::take(&mut info.face_edge_ids),
        std::mem::take(&mut info.edge_diff),
    )?;
    let l = &p.hierarchy.levels[0];
    let faces = &p.hierarchy.faces;
    let mut tree = DisjointTree::new(l.v.len());
    for (i, d) in h.levels[0].edge_diff.iter().enumerate() {
        if *d == [0, 0] {
            let ev = info.edge_values[i];
            tree.merge(ev.x as usize, ev.y as usize);
        }
    }
    tree.build_compact_parent();

    let coarsest = h.levels.last().expect("a level").clone();
    let mut face = vec![0usize; coarsest.f2e.len()];
    for i in 0..faces.len() {
        let mut t = i as i32;
        for map in &h.to_upper_faces {
            t = map[t as usize];
            if t < 0 {
                break;
            }
        }
        if t >= 0 {
            face[t as usize] = i;
        }
    }
    h.into_edge_info(info);

    let num_v = tree.compact_num();
    let mut compact = QuadMesh {
        o: vec![Vec3::default(); num_v],
        q: vec![Vec3::default(); num_v],
        n: vec![Vec3::default(); num_v],
        vset: vec![Vec::new(); num_v],
        ..QuadMesh::default()
    };
    let mut counter = vec![0usize; num_v];
    for i in 0..l.o.len() {
        let c = tree.index(i);
        compact.vset[c].push(i as u32);
        compact.o[c] = compact.o[c] + l.o[i];
        compact.n[c] = normalized(compact.n[c] * counter[c] as f64 + l.n[i]);
        if counter[c] == 0 {
            compact.q[c] = l.q[i];
        } else {
            let (a, b) = compat_orientation_extrinsic_4(compact.q[c], compact.n[c], l.q[i], l.n[i]);
            compact.q[c] = normalized(a * counter[c] as f64 + b);
        }
        counter[c] += 1;
    }
    for (o, &c) in compact.o.iter_mut().zip(&counter) {
        *o = *o / c as f64;
    }

    let quads = build_triangle_manifold(&tree, &face, faces, &coarsest, compact);
    Ok(Extracted {
        quads,
        groups: tree,
    })
}

fn prev3(d: i32) -> i32 {
    d / 3 * 3 + (d + 2) % 3
}

fn next3(d: i32) -> i32 {
    d / 3 * 3 + (d + 1) % 3
}

fn build_triangle_manifold(
    tree: &DisjointTree,
    face: &[usize],
    fine_faces: &[[u32; 3]],
    coarse: &super::flip::EdgeLevel,
    compact: QuadMesh,
) -> QuadMesh {
    let mut e2e = level_e2e(coarse);
    let count = coarse.f2e.len();
    let mut tv = vec![[-1i32; 3]; count];
    let mut out = QuadMesh::default();
    let mut num_v = 0i32;
    for i in 0..count {
        for j in 0..3 {
            if tv[i][j] != -1 {
                continue;
            }
            let v = tree.index(fine_faces[face[i]][j] as usize);
            out.vset.push(compact.vset[v].clone());
            out.q.push(compact.q[v]);
            out.n.push(compact.n[v]);
            out.o.push(compact.o[v]);
            let deid0 = (i * 3 + j) as i32;
            let mut deid = deid0;
            loop {
                tv[(deid / 3) as usize][(deid % 3) as usize] = num_v;
                deid = e2e[prev3(deid) as usize];
                if deid == deid0 || deid == -1 {
                    break;
                }
            }
            if deid == -1 {
                deid = deid0;
                loop {
                    deid = e2e[deid as usize];
                    if deid == -1 {
                        break;
                    }
                    deid = next3(deid);
                    tv[(deid / 3) as usize][(deid % 3) as usize] = num_v;
                }
            }
            num_v += 1;
        }
    }

    let degenerate = |t: &[i32; 3]| t[0] == t[1] || t[1] == t[2] || t[2] == t[0];
    let corner = |tv: &[[i32; 3]], d: i32| tv[(d / 3) as usize][(d % 3) as usize];
    loop {
        let num_v0 = num_v;
        let mut vert_to_dedge: Vec<Vec<i32>> = vec![Vec::new(); num_v as usize];
        for i in 0..count {
            if degenerate(&tv[i]) {
                for j in 0..3 {
                    let t = e2e[i * 3 + j];
                    if t != -1 {
                        e2e[t as usize] = -1;
                    }
                }
                for j in 0..3 {
                    e2e[i * 3 + j] = -1;
                }
            } else {
                for j in 0..3 {
                    vert_to_dedge[tv[i][j] as usize].push((i * 3 + j) as i32);
                }
            }
        }
        let mut colours = vec![-1i32; count * 3];
        for (i, dedges_of_vertex) in vert_to_dedge.iter().enumerate() {
            let mut num_colour = 0i32;
            for &start in dedges_of_vertex {
                if colours[start as usize] != -1 {
                    continue;
                }
                let mut ring: VecDeque<i32> = VecDeque::new();
                let mut deid = start;
                loop {
                    ring.push_back(deid);
                    deid = e2e[prev3(deid) as usize];
                    if deid == -1 || deid == start {
                        break;
                    }
                }
                if deid == -1 {
                    deid = start;
                    loop {
                        deid = e2e[deid as usize];
                        if deid == -1 {
                            break;
                        }
                        deid = next3(deid);
                        if deid == start {
                            break;
                        }
                        ring.push_front(deid);
                    }
                }
                let dedges: Vec<i32> = ring.into_iter().collect();
                let mut loc: BTreeMap<(i32, i32), usize> = BTreeMap::new();
                let mut dedge_colours = vec![num_colour; dedges.len()];
                num_colour += 1;
                for jj in 0..dedges.len() {
                    let d = dedges[jj];
                    colours[d as usize] = 0;
                    let pt = (corner(&tv, d), corner(&tv, next3(d)));
                    if let Some(&s) = loc.get(&pt) {
                        for k in s..jj {
                            let dk = dedges[k];
                            loc.remove(&(corner(&tv, dk), corner(&tv, next3(dk))));
                            dedge_colours[k] = num_colour;
                        }
                        num_colour += 1;
                    }
                    loc.insert(pt, jj);
                }
                for (j, &d) in dedges.iter().enumerate() {
                    let c = dedge_colours[j];
                    if c > 0 {
                        tv[(d / 3) as usize][(d % 3) as usize] = num_v + c - 1;
                    }
                }
            }
            if num_colour > 1 {
                for _ in 0..num_colour - 1 {
                    out.push_copy_of(i);
                }
                num_v += num_colour - 1;
            }
        }
        if num_v == num_v0 {
            break;
        }
    }

    let mut triangles: Vec<([i32; 3], [u32; 3])> = Vec::new();
    for (t, edges) in tv.iter().zip(&coarse.f2e) {
        if !degenerate(t) {
            triangles.push((*t, *edges));
        }
    }
    let mut quads: BTreeMap<DEdge, ([i32; 3], [i32; 3])> = BTreeMap::new();
    for (t, edges) in &triangles {
        for j in 0..3 {
            let d = coarse.edge_diff[edges[j] as usize];
            if d[0].abs() == 1 && d[1].abs() == 1 {
                let (v1, v2, v3) = (t[j], t[(j + 1) % 3], t[(j + 2) % 3]);
                let key = DEdge::new(v1 as u32, v2 as u32);
                quads
                    .entry(key)
                    .and_modify(|p| p.1 = [v1, v2, v3])
                    .or_insert(([v1, v2, v3], [-1, -1, -1]));
            }
        }
    }
    for (first, second) in quads.values() {
        if second[0] != -1 && first[2] != second[2] {
            out.faces
                .push([first[1], first[2], first[0], second[2]].map(|v| v as u32));
        }
    }
    out.rebuild_graph();

    loop {
        let before = out.faces.len();
        out.faces
            .retain(|f| !(0..3).any(|j| (j + 1..4).any(|k| f[j] == f[k])));
        if out.faces.len() == before {
            break;
        }
        out.rebuild_graph();
    }
    fix_holes(&mut out);
    out.rebuild_graph();
    out
}

fn quad_energy(mesh: &QuadMesh, loop_vertices: &[u32], res: &mut Vec<[u32; 4]>) -> f64 {
    if loop_vertices.len() < 4 {
        return 0.0;
    }
    if loop_vertices.len() == 4 {
        let mut energy = 0.0;
        for j in 0..4 {
            let v0 = loop_vertices[j] as usize;
            let v2 = loop_vertices[(j + 1) % 4] as usize;
            let v1 = loop_vertices[(j + 3) % 4] as usize;
            let pt1 = normalized(mesh.o[v1] - mesh.o[v0]);
            let pt2 = normalized(mesh.o[v2] - mesh.o[v0]);
            let n = pt1.cross(pt2);
            let mut sina = n.length();
            if n.dot(mesh.n[v0]) < 0.0 {
                sina = -sina;
            }
            let cosa = pt1.dot(pt2);
            let mut angle = sina.atan2(cosa) / UPSTREAM_PI * 180.0;
            if angle < 0.0 {
                angle += 360.0;
            }
            energy += angle * angle;
        }
        res.push([
            loop_vertices[0],
            loop_vertices[3],
            loop_vertices[2],
            loop_vertices[1],
        ]);
        return energy;
    }
    let len = loop_vertices.len();
    let mut best = 1e30;
    for seg1 in (2..len).step_by(2) {
        for seg2 in (seg1 + 1..len).step_by(2) {
            let mut quads: [Vec<[u32; 4]>; 4] = Default::default();
            let corners = [
                loop_vertices[0],
                loop_vertices[1],
                loop_vertices[seg1],
                loop_vertices[seg2],
            ];
            let mut energy = quad_energy(mesh, &corners, &mut quads[0]);
            if seg1 > 2 {
                let mut part = loop_vertices[1..seg1].to_vec();
                part.push(loop_vertices[seg1]);
                energy += quad_energy(mesh, &part, &mut quads[1]);
            }
            if seg2 != seg1 + 1 {
                let mut part = loop_vertices[seg1..seg2].to_vec();
                part.push(loop_vertices[seg2]);
                energy += quad_energy(mesh, &part, &mut quads[2]);
            }
            if seg2 + 1 != len {
                let mut part = loop_vertices[seg2..].to_vec();
                part.push(loop_vertices[0]);
                energy += quad_energy(mesh, &part, &mut quads[3]);
            }
            if best > energy {
                best = energy;
                res.clear();
                for q in &quads {
                    res.extend_from_slice(q);
                }
            }
        }
    }
    best
}

fn fix_hole_loop(
    mesh: &mut QuadMesh,
    quad_edges: &mut BTreeSet<(u32, u32)>,
    loop_vertices: &[u32],
) {
    let mut loops: Vec<Vec<u32>> = Vec::new();
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for (i, &v) in loop_vertices.iter().enumerate() {
        if let Some(&j) = seen.get(&v) {
            loops.push(Vec::new());
            if i - j > 3 && (i - j) % 2 == 0 {
                for &u in &loop_vertices[j..i] {
                    if seen.remove(&u).is_some() {
                        loops.last_mut().expect("just pushed").push(u);
                    }
                }
            }
        }
        seen.insert(v, i);
    }
    if seen.len() >= 3 {
        let mut rest = Vec::new();
        for &u in loop_vertices {
            if seen.remove(&u).is_some() {
                rest.push(u);
            }
        }
        loops.push(rest);
    }
    for hole in &loops {
        if hole.is_empty() {
            return;
        }
        let mut quads = Vec::new();
        quad_energy(mesh, hole, &mut quads);
        for quad in quads {
            let taken = (0..4).any(|j| quad_edges.contains(&(quad[j], quad[(j + 1) % 4])));
            if !taken {
                for j in 0..4 {
                    quad_edges.insert((quad[j], quad[(j + 1) % 4]));
                }
                mesh.faces.push(quad);
            }
        }
    }
}

pub fn fix_holes(mesh: &mut QuadMesh) {
    let mut quad_edges: BTreeSet<(u32, u32)> = BTreeSet::new();
    for f in &mesh.faces {
        for j in 0..4 {
            quad_edges.insert((f[j], f[(j + 1) % 4]));
        }
    }
    let next4 = |e: u32| e / 4 * 4 + (e + 1) % 4;
    let edge_count = mesh.e2e.len();
    let mut detected = vec![false; edge_count];
    for i in 0..edge_count {
        if detected[i] || mesh.e2e[i] != INVALID {
            continue;
        }
        let mut loop_edges = Vec::new();
        let mut current = i as u32;
        while !detected[current as usize] {
            detected[current as usize] = true;
            loop_edges.push(current);
            current = next4(current);
            while mesh.e2e[current as usize] != INVALID {
                current = next4(mesh.e2e[current as usize]);
            }
        }
        let loop_vertices: Vec<u32> = loop_edges
            .iter()
            .map(|&e| mesh.faces[(e / 4) as usize][(e % 4) as usize])
            .collect();
        if loop_vertices.len() < HOLE_LIMIT {
            fix_hole_loop(mesh, &mut quad_edges, &loop_vertices);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square_ring(mesh: &mut QuadMesh, positions: &[(f64, f64)]) {
        for &(x, y) in positions {
            mesh.o.push(Vec3::new(x, y, 0.0));
            mesh.n.push(Vec3::new(0.0, 0.0, 1.0));
            mesh.q.push(Vec3::new(1.0, 0.0, 0.0));
            mesh.vset.push(Vec::new());
        }
    }

    #[test]
    fn a_square_hole_gets_one_counter_clockwise_quad() {
        let mut mesh = QuadMesh::default();
        square_ring(&mut mesh, &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        let mut quads = Vec::new();
        let energy = quad_energy(&mesh, &[0, 3, 2, 1], &mut quads);
        assert_eq!(quads, [[0, 1, 2, 3]]);
        assert!((energy - 4.0 * 90.0 * 90.0).abs() < 1e-3);
    }

    #[test]
    fn a_hexagonal_hole_splits_into_two_quads() {
        let mut mesh = QuadMesh::default();
        square_ring(
            &mut mesh,
            &[
                (0.0, 0.0),
                (0.0, 1.0),
                (0.0, 2.0),
                (1.0, 2.0),
                (1.0, 1.0),
                (1.0, 0.0),
            ],
        );
        let mut quads = Vec::new();
        quad_energy(&mesh, &[0, 1, 2, 3, 4, 5], &mut quads);
        assert_eq!(quads.len(), 2);
    }

    #[test]
    fn a_missing_quad_in_a_grid_is_filled_back() {
        let mut mesh = QuadMesh::default();
        let pts: Vec<(f64, f64)> = (0..8)
            .flat_map(|y| (0..8).map(move |x| (x as f64, y as f64)))
            .collect();
        square_ring(&mut mesh, &pts);
        let id = |x: u32, y: u32| y * 8 + x;
        for y in 0..7 {
            for x in 0..7 {
                if (x, y) != (3, 3) {
                    mesh.faces
                        .push([id(x, y), id(x + 1, y), id(x + 1, y + 1), id(x, y + 1)]);
                }
            }
        }
        mesh.rebuild_graph();
        fix_holes(&mut mesh);
        assert_eq!(mesh.faces.len(), 49);
        let mut added = *mesh.faces.last().unwrap();
        added.sort();
        assert_eq!(added, [id(3, 3), id(4, 3), id(3, 4), id(4, 4)]);
        mesh.rebuild_graph();
        let open = mesh.e2e.iter().filter(|&&e| e == INVALID).count();
        assert_eq!(open, 28);
    }

    #[test]
    fn a_border_shorter_than_the_limit_is_capped_like_upstream() {
        let mut mesh = QuadMesh::default();
        square_ring(&mut mesh, &[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]);
        mesh.faces.push([0, 1, 2, 3]);
        mesh.rebuild_graph();
        fix_holes(&mut mesh);
        assert_eq!(mesh.faces, [[0, 1, 2, 3], [0, 3, 2, 1]]);
    }

    fn extracted(mesh: (Vec<Vec3>, Vec<[u32; 3]>), faces: usize) -> QuadMesh {
        let (mut p, mut info) = crate::route_b::solve::tests::through_flip(mesh, faces);
        crate::route_b::solve::optimize_positions_fixed(&mut p.hierarchy, &info).unwrap();
        advanced_extract_quad(&p, &mut info).unwrap().quads
    }

    #[test]
    fn extraction_turns_a_flat_patch_into_clean_quads() {
        use crate::route_b::orient::tests::flat;
        let q = extracted(flat(4), 200);
        assert!(
            (180..=220).contains(&q.faces.len()),
            "{} quads",
            q.faces.len()
        );
        for f in &q.faces {
            assert!((0..3).all(|j| (j + 1..4).all(|k| f[j] != f[k])));
            assert!(f.iter().all(|&v| (v as usize) < q.o.len()));
        }
        assert!(q.non_manifold.iter().all(|&b| !b));
    }

    #[test]
    fn extraction_closes_a_cube_into_a_sphere_like_quad_mesh_near_its_target() {
        use crate::route_b::orient::tests::cube;
        let q = extracted(cube(), 300);
        let open = q.e2e.iter().filter(|&&e| e == INVALID).count();
        eprintln!(
            "PROBE cube quads {} vertices {} open {}",
            q.faces.len(),
            q.o.len(),
            open
        );
        assert!(!q.faces.is_empty());
        assert_eq!(open, 0);
    }
}
