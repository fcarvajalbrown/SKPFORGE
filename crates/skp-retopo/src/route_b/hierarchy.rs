use super::adjacency::{Adjacency, Link};
use super::dedge::INVALID;
use super::field_math::coordinate_system;
use super::pcg32::Pcg32;
use skp_core::geometry::Vec3;
use std::cmp::Ordering;
use std::f64::consts::PI;

pub const MAX_DEPTH: usize = 25;
pub const RCP_OVERFLOW: f64 = f64::from_bits(0x37f0_0000_0000_0000);

#[derive(Debug, Clone, Default)]
pub struct Level {
    pub v: Vec<Vec3>,
    pub n: Vec<Vec3>,
    pub a: Vec<f64>,
    pub adj: Adjacency,
    pub phases: Vec<Vec<u32>>,
    pub q: Vec<Vec3>,
    pub o: Vec<Vec3>,
}

#[derive(Debug, Clone, Default)]
pub struct Hierarchy {
    pub levels: Vec<Level>,
    pub to_upper: Vec<Vec<[u32; 2]>>,
    pub to_lower: Vec<Vec<u32>>,
    pub scale: f64,
}

pub fn graph_colouring(adj: &Adjacency) -> Vec<Vec<u32>> {
    let size = adj.len();
    let mut colour = vec![usize::MAX; size];
    let mut possible: Vec<bool> = Vec::new();
    let mut size_per_colour: Vec<usize> = Vec::new();
    for i in 0..size {
        possible.fill(true);
        for link in &adj[i] {
            let c = colour[link.id as usize];
            if c != usize::MAX {
                possible[c] = false;
            }
        }
        let chosen = match possible.iter().position(|&p| p) {
            Some(c) => c,
            None => {
                possible.push(false);
                size_per_colour.push(0);
                possible.len() - 1
            }
        };
        colour[i] = chosen;
        size_per_colour[chosen] += 1;
    }
    let mut phases: Vec<Vec<u32>> = size_per_colour
        .iter()
        .map(|&n| Vec::with_capacity(n))
        .collect();
    for (i, &c) in colour.iter().enumerate() {
        phases[c].push(i as u32);
    }
    phases
}

struct Entry {
    i: u32,
    j: u32,
    order: f64,
}

fn highest_order_first(a: &Entry, b: &Entry) -> Ordering {
    match (a.order.is_nan(), b.order.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => b.order.partial_cmp(&a.order).unwrap_or(Ordering::Equal),
    }
}

pub struct Downsampled {
    pub level: Level,
    pub to_upper: Vec<[u32; 2]>,
    pub to_lower: Vec<u32>,
}

pub fn downsample(fine: &Level) -> Downsampled {
    let (v, n, a, adj) = (&fine.v, &fine.n, &fine.a, &fine.adj);
    let mut entries: Vec<Entry> = Vec::new();
    for (i, links) in adj.iter().enumerate() {
        for link in links {
            let k = link.id as usize;
            let dp = n[i].dot(n[k]);
            let ratio = if a[i] > a[k] {
                a[i] / a[k]
            } else {
                a[k] / a[i]
            };
            entries.push(Entry {
                i: i as u32,
                j: link.id,
                order: dp * ratio,
            });
        }
    }
    entries.sort_by(highest_order_first);

    let mut merged = vec![false; v.len()];
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for e in &entries {
        let (i, j) = (e.i as usize, e.j as usize);
        if merged[i] || merged[j] {
            continue;
        }
        merged[i] = true;
        merged[j] = true;
        pairs.push((i, j));
    }

    let count = v.len() - pairs.len();
    let mut level = Level {
        v: Vec::with_capacity(count),
        n: Vec::with_capacity(count),
        a: Vec::with_capacity(count),
        ..Level::default()
    };
    let mut to_upper = Vec::with_capacity(count);
    let mut to_lower = vec![INVALID; v.len()];

    for (idx, &(i, j)) in pairs.iter().enumerate() {
        let (area1, area2) = (a[i], a[j]);
        let surface_area = area1 + area2;
        level.v.push(if surface_area > RCP_OVERFLOW {
            (v[i] * area1 + v[j] * area2) / surface_area
        } else {
            (v[i] + v[j]) * 0.5
        });
        let normal = n[i] * area1 + n[j] * area2;
        let norm = normal.length();
        level.n.push(if norm > RCP_OVERFLOW {
            normal / norm
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        });
        level.a.push(surface_area);
        to_upper.push([i as u32, j as u32]);
        to_lower[i] = idx as u32;
        to_lower[j] = idx as u32;
    }
    for i in 0..v.len() {
        if !merged[i] {
            to_lower[i] = level.v.len() as u32;
            level.v.push(v[i]);
            level.n.push(n[i]);
            level.a.push(a[i]);
            to_upper.push([i as u32, INVALID]);
        }
    }

    level.adj = to_upper
        .iter()
        .enumerate()
        .map(|(i, upper)| {
            let mut scratch: Vec<Link> = upper
                .iter()
                .filter(|&&u| u != INVALID)
                .flat_map(|&u| adj[u as usize].iter())
                .map(|l| Link {
                    id: to_lower[l.id as usize],
                    weight: l.weight,
                })
                .collect();
            scratch.sort_by_key(|l| l.id);
            let mut merged_links: Vec<Link> = Vec::new();
            for link in scratch {
                if link.id == i as u32 {
                    continue;
                }
                match merged_links.last_mut() {
                    Some(last) if last.id == link.id => last.weight += link.weight,
                    _ => merged_links.push(link),
                }
            }
            merged_links
        })
        .collect();

    Downsampled {
        level,
        to_upper,
        to_lower,
    }
}

impl Hierarchy {
    pub fn build(finest: Level, scale: f64, rng: &mut Pcg32) -> Hierarchy {
        let mut h = Hierarchy {
            levels: vec![finest],
            scale,
            ..Hierarchy::default()
        };
        h.levels[0].phases = graph_colouring(&h.levels[0].adj);
        for _ in 0..MAX_DEPTH {
            let mut next = downsample(h.levels.last().expect("finest level"));
            next.level.phases = graph_colouring(&next.level.adj);
            let single = next.level.v.len() == 1;
            h.levels.push(next.level);
            h.to_upper.push(next.to_upper);
            h.to_lower.push(next.to_lower);
            if single {
                break;
            }
        }

        for level in &mut h.levels {
            let count = level.n.len();
            level.q = Vec::with_capacity(count);
            level.o = Vec::with_capacity(count);
            for j in 0..count {
                let (s, t) = coordinate_system(level.n[j]);
                let angle = rng.next_f64() * 2.0 * PI;
                let x = rng.next_f64() * 2.0 - 1.0;
                let y = rng.next_f64() * 2.0 - 1.0;
                level.q.push(s * angle.cos() + t * angle.sin());
                level.o.push(level.v[j] + (s * x + t * y) * scale);
            }
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links(ids: &[u32]) -> Vec<Link> {
        ids.iter().map(|&id| Link::new(id)).collect()
    }

    fn path_level(areas: [f64; 4]) -> Level {
        Level {
            v: (0..4).map(|i| Vec3::new(i as f64, 0.0, 0.0)).collect(),
            n: vec![Vec3::new(0.0, 0.0, 1.0); 4],
            a: areas.to_vec(),
            adj: vec![links(&[1]), links(&[0, 2]), links(&[1, 3]), links(&[2])],
            ..Level::default()
        }
    }

    fn grid_level(side: u32) -> Level {
        let id = |x: u32, y: u32| y * side + x;
        let mut level = Level::default();
        for y in 0..side {
            for x in 0..side {
                level.v.push(Vec3::new(x as f64, y as f64, 0.0));
                level.n.push(Vec3::new(0.0, 0.0, 1.0));
                level.a.push(1.0);
                let mut ns = Vec::new();
                if x > 0 {
                    ns.push(id(x - 1, y));
                }
                if x + 1 < side {
                    ns.push(id(x + 1, y));
                }
                if y > 0 {
                    ns.push(id(x, y - 1));
                }
                if y + 1 < side {
                    ns.push(id(x, y + 1));
                }
                level.adj.push(links(&ns));
            }
        }
        level
    }

    #[test]
    fn colouring_is_greedy_in_vertex_order() {
        let adj = vec![links(&[1]), links(&[0, 2]), links(&[1, 3]), links(&[2])];
        assert_eq!(graph_colouring(&adj), [vec![0, 2], vec![1, 3]]);
        let triangle = vec![links(&[1, 2]), links(&[0, 2]), links(&[0, 1])];
        assert_eq!(graph_colouring(&triangle), [vec![0], vec![1], vec![2]]);
    }

    #[test]
    fn no_two_neighbours_share_a_colour() {
        let level = grid_level(7);
        let phases = graph_colouring(&level.adj);
        let mut colour = vec![0; level.v.len()];
        for (c, phase) in phases.iter().enumerate() {
            for &v in phase {
                colour[v as usize] = c;
            }
        }
        for (i, links) in level.adj.iter().enumerate() {
            assert!(links.iter().all(|l| colour[l.id as usize] != colour[i]));
        }
    }

    #[test]
    fn downsampling_merges_the_most_unequal_areas_first() {
        let d = downsample(&path_level([1.0, 2.0, 2.0, 1.0]));
        assert_eq!(d.to_upper, [[0, 1], [2, 3]]);
        assert_eq!(d.to_lower, [0, 0, 1, 1]);
        assert_eq!(d.level.v[0], Vec3::new(2.0 / 3.0, 0.0, 0.0));
        assert_eq!(d.level.v[1], Vec3::new(7.0 / 3.0, 0.0, 0.0));
        assert_eq!(d.level.a, [3.0, 3.0]);
        assert_eq!(d.level.adj, [links(&[1]), links(&[0])]);
    }

    #[test]
    fn unmerged_vertices_follow_the_merged_pairs() {
        let level = Level {
            v: (0..3).map(|i| Vec3::new(i as f64, 0.0, 0.0)).collect(),
            n: vec![Vec3::new(0.0, 0.0, 1.0); 3],
            a: vec![1.0, 1.0, 1.0],
            adj: vec![links(&[1]), links(&[0, 2]), links(&[1])],
            ..Level::default()
        };
        let d = downsample(&level);
        assert_eq!(d.to_upper, [[0, 1], [2, INVALID]]);
        assert_eq!(d.level.adj[0], [Link { id: 1, weight: 1.0 }]);
        assert_eq!(d.level.adj[1], [Link { id: 0, weight: 1.0 }]);
    }

    #[test]
    fn shared_neighbours_add_their_weights() {
        let d = downsample(&grid_level(2));
        assert_eq!(d.level.v.len(), 2);
        assert_eq!(d.level.adj[0], [Link { id: 1, weight: 2.0 }]);
    }

    #[test]
    fn the_hierarchy_ends_at_a_single_vertex() {
        let h = Hierarchy::build(grid_level(9), 0.5, &mut Pcg32::default());
        let counts: Vec<usize> = h.levels.iter().map(|l| l.v.len()).collect();
        assert_eq!(counts.first(), Some(&81));
        assert_eq!(counts.last(), Some(&1));
        assert!(counts.windows(2).all(|w| w[1] < w[0]));
        assert_eq!(h.to_upper.len(), h.levels.len() - 1);
        let total: f64 = h.levels.last().unwrap().a.iter().sum();
        assert_eq!(total, 81.0);
    }

    #[test]
    fn the_random_field_is_tangent_and_within_one_scale_of_each_vertex() {
        let h = Hierarchy::build(grid_level(5), 0.5, &mut Pcg32::seeded(7, 1));
        for level in &h.levels {
            for j in 0..level.v.len() {
                let (q, n, d) = (level.q[j], level.n[j], level.o[j] - level.v[j]);
                assert!((q.length() - 1.0).abs() < 1e-12);
                assert!(q.dot(n).abs() < 1e-12);
                assert!(d.dot(n).abs() < 1e-12);
                assert!(d.x.abs() <= 0.5 && d.y.abs() <= 0.5);
            }
        }
        let again = Hierarchy::build(grid_level(5), 0.5, &mut Pcg32::seeded(7, 1));
        assert_eq!(again.levels[2].o, h.levels[2].o);
    }
}
