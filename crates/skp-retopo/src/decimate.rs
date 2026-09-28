use crate::error::RetopoError;
use crate::route::SHARP_ANGLE_DEGREES;
use crate::surface::{same_corner, Surface, Triangles};
use skp_core::correspondence::{Correspondence, CorrespondenceBuilder};
use skp_core::geometry::{area_vector, height, Vec3};
use skp_core::mesh::{Face, FaceData, Mesh};
use skp_core::progress::{CancelToken, Progress, ProgressSink};
use skp_core::topology::Incidence;
use skp_core::units::Uu;
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fmt;

const STAGES: u64 = 3;
const CANCEL_EVERY: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DecimateReport {
    pub high_triangles: usize,
    pub target_triangles: usize,
    pub vertices: usize,
    pub pinned_vertices: usize,
    pub locked: LockedEdges,
    pub collapses: usize,
    pub triangles: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LockedEdges {
    pub boundary: usize,
    pub inconsistent: usize,
    pub material: usize,
    pub uv_seam: usize,
    pub normal_seam: usize,
    pub sharp: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lock {
    Boundary,
    Inconsistent,
    Material,
    UvSeam,
    NormalSeam,
    Sharp,
}

impl LockedEdges {
    fn count(&mut self, lock: Lock) {
        let slot = match lock {
            Lock::Boundary => &mut self.boundary,
            Lock::Inconsistent => &mut self.inconsistent,
            Lock::Material => &mut self.material,
            Lock::UvSeam => &mut self.uv_seam,
            Lock::NormalSeam => &mut self.normal_seam,
            Lock::Sharp => &mut self.sharp,
        };
        *slot += 1;
    }
}

#[derive(Debug, Clone)]
pub struct Decimated {
    pub mesh: Mesh,
    pub correspondence: Correspondence,
    pub report: DecimateReport,
}

#[derive(Debug, Clone, Copy, Default)]
struct Quadric([f64; 10]);

impl Quadric {
    fn of_plane(normal: Vec3, point: Vec3, weight: f64) -> Quadric {
        let Vec3 { x: a, y: b, z: c } = normal;
        let d = -normal.dot(point);
        Quadric(
            [
                a * a,
                a * b,
                a * c,
                a * d,
                b * b,
                b * c,
                b * d,
                c * c,
                c * d,
                d * d,
            ]
            .map(|v| v * weight),
        )
    }

    fn plus(self, other: Quadric) -> Quadric {
        let mut sum = self.0;
        for (s, o) in sum.iter_mut().zip(other.0) {
            *s += o;
        }
        Quadric(sum)
    }

    fn error(&self, p: Vec3) -> f64 {
        let [aa, ab, ac, ad, bb, bc, bd, cc, cd, dd] = self.0;
        let Vec3 { x, y, z } = p;
        aa * x * x
            + bb * y * y
            + cc * z * z
            + 2.0 * (ab * x * y + ac * x * z + bc * y * z)
            + 2.0 * (ad * x + bd * y + cd * z)
            + dd
    }
}

#[derive(Debug, Clone, Copy)]
struct Collapse {
    cost: f64,
    from: u32,
    to: u32,
    stamps: (u32, u32),
}

impl Ord for Collapse {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then(other.from.cmp(&self.from))
            .then(other.to.cmp(&self.to))
    }
}

impl PartialOrd for Collapse {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Collapse {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Collapse {}

#[derive(Debug, Clone, Copy)]
struct Spoke {
    triangle: u32,
    first: u32,
    second: u32,
}

struct Plan {
    fan: Vec<Spoke>,
    wing: usize,
    corner: u32,
}

struct State<'a> {
    mesh: &'a Mesh,
    points: Vec<Vec3>,
    corners: Vec<[u32; 3]>,
    data: Vec<FaceData>,
    alive: Vec<bool>,
    around: Vec<Vec<u32>>,
    quadrics: Vec<Quadric>,
    pinned: Vec<bool>,
    locked: LockedEdges,
    stamps: Vec<u32>,
    absorbed: Vec<Vec<u32>>,
    around_count: usize,
    tolerance: f64,
    limit: f64,
}

pub fn decimate(
    mesh: &Mesh,
    target: usize,
    tolerance: Uu,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> Result<Decimated, RetopoError> {
    mesh.validate()?;
    cancel.check()?;
    if target == 0 {
        return Err(RetopoError::ZeroTarget);
    }
    let surface = Surface::of(mesh);
    let count = surface.triangles.len();
    if count == 0 {
        return Err(RetopoError::EmptyMesh);
    }
    let mut state = State::of(mesh, &surface, tolerance);
    step(1, cancel, progress)?;

    let mut heap = BinaryHeap::new();
    for (edge, incidences) in surface.edges.iter() {
        if incidences.len() == 2 {
            for (from, to) in [edge, (edge.1, edge.0)] {
                heap.extend(state.collapse(from, to));
            }
        }
    }
    let mut remaining = count;
    let mut collapses = 0;
    let mut popped = 0usize;
    while remaining > target {
        let Some(next) = heap.pop() else {
            break;
        };
        popped += 1;
        if popped.is_multiple_of(CANCEL_EVERY) {
            cancel.check()?;
        }
        if (
            state.stamps[next.from as usize],
            state.stamps[next.to as usize],
        ) != next.stamps
        {
            continue;
        }
        let Some(plan) = state.plan(next.from, next.to) else {
            continue;
        };
        state.apply(plan, next.from, next.to);
        remaining -= 2;
        collapses += 1;
        for w in state.neighbours(next.to) {
            for (from, to) in [(next.to, w), (w, next.to)] {
                heap.extend(state.collapse(from, to));
            }
        }
    }
    step(2, cancel, progress)?;

    let mut builder = CorrespondenceBuilder::new(remaining);
    let mut faces = Vec::with_capacity(remaining);
    let mut face_data = Vec::with_capacity(remaining);
    let mut low = 0u32;
    for t in 0..count {
        if !state.alive[t] {
            continue;
        }
        faces.push(Face::Tri(state.corners[t]));
        face_data.push(state.data[t]);
        let mut highs = state.absorbed[t].clone();
        highs.sort_unstable();
        for high in highs {
            builder.push(low, high);
        }
        low += 1;
    }
    let decimated = Mesh {
        faces,
        face_data,
        ..mesh.clone()
    };
    decimated.validate()?;
    let correspondence = builder.build();
    correspondence.validate()?;
    step(3, cancel, progress)?;

    Ok(Decimated {
        report: DecimateReport {
            high_triangles: count,
            target_triangles: target,
            vertices: state.around_count,
            pinned_vertices: state.pinned.iter().filter(|&&p| p).count(),
            locked: state.locked,
            collapses,
            triangles: remaining,
        },
        mesh: decimated,
        correspondence,
    })
}

fn step(done: u64, cancel: &CancelToken, progress: &dyn ProgressSink) -> Result<(), RetopoError> {
    progress.report(Progress::Measured {
        done,
        total: STAGES,
    });
    cancel.check()?;
    Ok(())
}

fn centre(points: &[Vec3]) -> Vec3 {
    let mut low = Vec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut high = low * -1.0;
    for p in points {
        low = Vec3::new(low.x.min(p.x), low.y.min(p.y), low.z.min(p.z));
        high = Vec3::new(high.x.max(p.x), high.y.max(p.y), high.z.max(p.z));
    }
    (low + high) * 0.5
}

fn locked(
    mesh: &Mesh,
    surface: &Surface,
    triangles: &Triangles,
    edge: (u32, u32),
    incidences: &[Incidence],
    limit: f64,
) -> Option<Lock> {
    let [a, b] = incidences else {
        return Some(Lock::Boundary);
    };
    if a.forward == b.forward {
        return Some(Lock::Inconsistent);
    }
    if triangles.data[a.face as usize] != triangles.data[b.face as usize] {
        return Some(Lock::Material);
    }
    let mut pairs = Vec::with_capacity(2);
    for position in [edge.0, edge.1] {
        let (Some(here), Some(there)) = (
            triangles.corner_at(mesh, a.face, position),
            triangles.corner_at(mesh, b.face, position),
        ) else {
            return Some(Lock::Boundary);
        };
        pairs.push((&mesh.corners[here as usize], &mesh.corners[there as usize]));
    }
    if pairs
        .iter()
        .any(|(x, y)| x.uvq != y.uvq || x.back_uvq != y.back_uvq)
    {
        return Some(Lock::UvSeam);
    }
    if pairs.iter().any(|(x, y)| x.normal != y.normal) {
        return Some(Lock::NormalSeam);
    }
    match (
        surface.normals[a.face as usize],
        surface.normals[b.face as usize],
    ) {
        (Some(na), Some(nb)) if na.dot(nb) >= limit => None,
        _ => Some(Lock::Sharp),
    }
}

impl<'a> State<'a> {
    fn of(mesh: &'a Mesh, surface: &Surface, tolerance: Uu) -> State<'a> {
        let triangles = Triangles::of(mesh);
        let raw: Vec<Vec3> = mesh.positions.iter().map(|&p| Vec3::of(p)).collect();
        let origin = centre(&raw);
        let points: Vec<Vec3> = raw.into_iter().map(|p| p - origin).collect();
        let limit = SHARP_ANGLE_DEGREES.to_radians().cos();
        let mut pinned = vec![false; points.len()];
        let mut counts = LockedEdges::default();
        for (edge, incidences) in surface.edges.iter() {
            if let Some(lock) = locked(mesh, surface, &triangles, edge, incidences, limit) {
                counts.count(lock);
                pinned[edge.0 as usize] = true;
                pinned[edge.1 as usize] = true;
            }
        }
        let mut quadrics = vec![Quadric::default(); points.len()];
        let mut around = vec![Vec::new(); points.len()];
        for (t, tri) in surface.triangles.iter().enumerate() {
            let [a, b, c] = tri.map(|p| points[p as usize]);
            let area = area_vector(a, b, c);
            let plane = area
                .normalised()
                .map(|n| Quadric::of_plane(n, a, area.length()));
            for &p in tri {
                if let Some(plane) = plane {
                    quadrics[p as usize] = quadrics[p as usize].plus(plane);
                }
                around[p as usize].push(t as u32);
            }
        }
        let count = surface.triangles.len();
        let around_count = around.iter().filter(|a| !a.is_empty()).count();
        State {
            mesh,
            stamps: vec![0; points.len()],
            points,
            corners: triangles.corners,
            data: triangles.data,
            alive: vec![true; count],
            around,
            quadrics,
            pinned,
            locked: counts,
            around_count,
            absorbed: (0..count as u32).map(|t| vec![t]).collect(),
            tolerance: tolerance.0,
            limit,
        }
    }

    fn position(&self, corner: u32) -> u32 {
        self.mesh.corners[corner as usize].position
    }

    fn positions(&self, triangle: u32) -> [u32; 3] {
        self.corners[triangle as usize].map(|c| self.position(c))
    }

    fn corner_at(&self, triangle: u32, position: u32) -> Option<u32> {
        self.corners[triangle as usize]
            .into_iter()
            .find(|&c| self.position(c) == position)
    }

    fn same_corner_at(&self, a: u32, b: u32, position: u32) -> Option<bool> {
        let here = self.corner_at(a, position)?;
        let there = self.corner_at(b, position)?;
        Some(same_corner(
            &self.mesh.corners[here as usize],
            &self.mesh.corners[there as usize],
        ))
    }

    fn normal(&self, positions: [u32; 3]) -> Option<Vec3> {
        let [a, b, c] = positions.map(|p| self.points[p as usize]);
        area_vector(a, b, c).normalised()
    }

    fn height(&self, positions: [u32; 3]) -> f64 {
        let [a, b, c] = positions.map(|p| self.points[p as usize]);
        height(a, b, c)
    }

    fn living(&self, position: u32) -> impl Iterator<Item = u32> + '_ {
        self.around[position as usize]
            .iter()
            .copied()
            .filter(|&t| self.alive[t as usize])
    }

    fn neighbours(&self, position: u32) -> Vec<u32> {
        let mut near: Vec<u32> = self
            .living(position)
            .flat_map(|t| self.positions(t))
            .filter(|&p| p != position)
            .collect();
        near.sort_unstable();
        near.dedup();
        near
    }

    fn collapse(&self, from: u32, to: u32) -> Option<Collapse> {
        if self.pinned[from as usize] {
            return None;
        }
        let quadric = self.quadrics[from as usize].plus(self.quadrics[to as usize]);
        Some(Collapse {
            cost: quadric.error(self.points[to as usize]),
            from,
            to,
            stamps: (self.stamps[from as usize], self.stamps[to as usize]),
        })
    }

    fn fan(&self, hub: u32) -> Option<Vec<Spoke>> {
        let mut spokes: Vec<Spoke> = self
            .living(hub)
            .map(|triangle| {
                let [a, b, c] = self.positions(triangle);
                let (first, second) = if a == hub {
                    (b, c)
                } else if b == hub {
                    (c, a)
                } else {
                    (a, b)
                };
                Spoke {
                    triangle,
                    first,
                    second,
                }
            })
            .collect();
        let n = spokes.len();
        if n < 3 {
            return None;
        }
        spokes.sort_unstable_by_key(|s| s.first);
        if spokes.windows(2).any(|w| w[0].first == w[1].first) {
            return None;
        }
        let mut order = Vec::with_capacity(n);
        let mut at = 0;
        for _ in 0..n {
            order.push(at);
            at = spokes
                .binary_search_by_key(&spokes[at].second, |s| s.first)
                .ok()?;
            if at == 0 && order.len() < n {
                return None;
            }
        }
        if at != 0 {
            return None;
        }
        Some(order.into_iter().map(|i| spokes[i]).collect())
    }

    fn plan(&self, u: u32, v: u32) -> Option<Plan> {
        if self.pinned[u as usize] {
            return None;
        }
        let fan = self.fan(u)?;
        let n = fan.len();
        let wing = fan.iter().position(|s| s.first == v)?;
        let other = (wing + n - 1) % n;
        let reference = fan[0].triangle;
        let data = self.data[reference as usize];
        for (i, spoke) in fan.iter().enumerate() {
            let next = fan[(i + 1) % n];
            if self.data[spoke.triangle as usize] != data
                || !self.same_corner_at(reference, spoke.triangle, u)?
                || !self.same_corner_at(spoke.triangle, next.triangle, spoke.second)?
            {
                return None;
            }
            let here = self.normal(self.positions(spoke.triangle))?;
            let there = self.normal(self.positions(next.triangle))?;
            if here.dot(there) < self.limit {
                return None;
            }
        }
        let near = self.neighbours(v);
        let shared = fan
            .iter()
            .filter(|s| s.first != v && near.binary_search(&s.first).is_ok())
            .count();
        if shared != 2 {
            return None;
        }
        for (i, spoke) in fan.iter().enumerate() {
            if i == wing || i == other {
                continue;
            }
            let before = self.normal([u, spoke.first, spoke.second])?;
            let moved = [v, spoke.first, spoke.second];
            let after = self.normal(moved)?;
            if after.dot(before) < self.limit || self.height(moved) < self.tolerance {
                return None;
            }
            if self.living(v).any(|t| {
                let p = self.positions(t);
                p.contains(&spoke.first) && p.contains(&spoke.second)
            }) {
                return None;
            }
        }
        let corner = self.corner_at(fan[wing].triangle, v)?;
        Some(Plan { fan, wing, corner })
    }

    fn apply(&mut self, plan: Plan, u: u32, v: u32) {
        let n = plan.fan.len();
        let other = (plan.wing + n - 1) % n;
        for (i, spoke) in plan.fan.iter().enumerate() {
            if i == plan.wing || i == other {
                continue;
            }
            for c in self.corners[spoke.triangle as usize].iter_mut() {
                if self.mesh.corners[*c as usize].position == u {
                    *c = plan.corner;
                }
            }
            self.around[v as usize].push(spoke.triangle);
        }
        for (wing, receiver) in [
            (plan.wing, (plan.wing + 1) % n),
            (other, (other + n - 1) % n),
        ] {
            let gone = plan.fan[wing].triangle as usize;
            let kept = plan.fan[receiver].triangle as usize;
            let moved = std::mem::take(&mut self.absorbed[gone]);
            self.absorbed[kept].extend(moved);
            self.alive[gone] = false;
        }
        self.quadrics[v as usize] = self.quadrics[v as usize].plus(self.quadrics[u as usize]);
        self.stamps[u as usize] += 1;
        self.stamps[v as usize] += 1;
        let alive = &self.alive;
        self.around[v as usize].retain(|&t| alive[t as usize]);
        self.around[u as usize].clear();
    }
}

impl fmt::Display for DecimateReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "pinned vertices        {} of {}",
            self.pinned_vertices, self.vertices
        )?;
        let l = self.locked;
        writeln!(
            f,
            "locked edges           {} open or non-manifold, {} inconsistent, {} material, {} uv seam, {} normal seam, {} sharp",
            l.boundary, l.inconsistent, l.material, l.uv_seam, l.normal_seam, l.sharp
        )?;
        writeln!(f, "collapses              {}", self.collapses)?;
        write!(f, "decimated triangles    {}", self.triangles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skp_core::mesh::{Corner, MaterialId, Normal, Point, Uvq};
    use skp_core::progress::NoProgress;
    use skp_core::topology::edge_counts;

    fn build(points: &[Point], triangles: &[[u32; 3]], data: &[FaceData]) -> Mesh {
        let mut mesh = Mesh {
            positions: points.to_vec(),
            ..Mesh::default()
        };
        for (tri, face_data) in triangles.iter().zip(data) {
            let first = mesh.corners.len() as u32;
            for &position in tri {
                let p = points[position as usize];
                mesh.corners.push(Corner {
                    position,
                    uvq: Uvq {
                        u: p.x.0,
                        v: p.y.0,
                        q: 1.0,
                    },
                    back_uvq: Uvq::default(),
                    normal: Normal::default(),
                });
            }
            mesh.faces.push(Face::Tri([first, first + 1, first + 2]));
            mesh.face_data.push(*face_data);
        }
        mesh
    }

    fn grid(n: u32, data: impl Fn(u32, u32) -> FaceData) -> Mesh {
        let mut points = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                points.push(Point::new(x as f64, y as f64, 0.0));
            }
        }
        let at = |x: u32, y: u32| y * (n + 1) + x;
        let mut triangles = Vec::new();
        let mut face_data = Vec::new();
        for y in 0..n {
            for x in 0..n {
                triangles.push([at(x, y), at(x + 1, y), at(x + 1, y + 1)]);
                triangles.push([at(x, y), at(x + 1, y + 1), at(x, y + 1)]);
                face_data.extend([data(x, y); 2]);
            }
        }
        build(&points, &triangles, &face_data)
    }

    fn sphere(segments: u32, rings: u32) -> Mesh {
        let mut points = vec![Point::new(0.0, 0.0, 10.0), Point::new(0.0, 0.0, -10.0)];
        for i in 1..rings {
            let polar = std::f64::consts::PI * i as f64 / rings as f64;
            for j in 0..segments {
                let around = std::f64::consts::TAU * j as f64 / segments as f64;
                points.push(Point::new(
                    10.0 * polar.sin() * around.cos(),
                    10.0 * polar.sin() * around.sin(),
                    10.0 * polar.cos(),
                ));
            }
        }
        let at = |i: u32, j: u32| 2 + (i - 1) * segments + j % segments;
        let mut triangles = Vec::new();
        for j in 0..segments {
            triangles.push([0, at(1, j), at(1, j + 1)]);
            triangles.push([1, at(rings - 1, j + 1), at(rings - 1, j)]);
            for i in 1..rings - 1 {
                let (a, b) = (at(i, j), at(i, j + 1));
                let (c, d) = (at(i + 1, j), at(i + 1, j + 1));
                triangles.push([a, c, d]);
                triangles.push([a, d, b]);
            }
        }
        let count = triangles.len();
        let mut mesh = build(&points, &triangles, &vec![FaceData::default(); count]);
        for corner in &mut mesh.corners {
            corner.uvq = Uvq::default();
        }
        mesh
    }

    fn run(mesh: &Mesh, target: usize) -> Decimated {
        decimate(
            mesh,
            target,
            skp_core::mesh::DEFAULT_WELD_TOLERANCE,
            &CancelToken::new(),
            &NoProgress,
        )
        .unwrap()
    }

    fn used_positions(mesh: &Mesh) -> Vec<u32> {
        let mut used: Vec<u32> = mesh
            .faces
            .iter()
            .flat_map(|f| f.corners().to_vec())
            .map(|c| mesh.corners[c as usize].position)
            .collect();
        used.sort_unstable();
        used.dedup();
        used
    }

    fn assert_partition(high: &Mesh, done: &Decimated) {
        let mut seen = vec![0usize; high.triangle_count()];
        for low in 0..done.mesh.triangle_count() as u32 {
            for &h in done.correspondence.high_for(low) {
                seen[h as usize] += 1;
                assert_eq!(
                    done.mesh.face_data[low as usize],
                    high.face_data[h as usize]
                );
            }
        }
        assert!(seen.iter().all(|&n| n == 1), "{seen:?}");
    }

    fn normals(mesh: &Mesh) -> Vec<Vec3> {
        (0..mesh.faces.len())
            .map(|f| {
                let [a, b, c] = skp_core::geometry::triangle_points(mesh, f);
                area_vector(a, b, c).normalised().unwrap()
            })
            .collect()
    }

    fn border(n: u32) -> Vec<u32> {
        (0..(n + 1) * (n + 1))
            .filter(|p| {
                let (x, y) = (p % (n + 1), p / (n + 1));
                x == 0 || y == 0 || x == n || y == n
            })
            .collect()
    }

    #[test]
    fn a_flat_grid_loses_interior_vertices_and_keeps_its_border() {
        let mesh = grid(5, |_, _| FaceData::default());
        let done = run(&mesh, 1);
        assert!(done.report.collapses > 0);
        assert_eq!(done.report.triangles, done.mesh.triangle_count());
        assert_eq!(done.report.triangles, 50 - 2 * done.report.collapses);
        let used = used_positions(&done.mesh);
        assert!(border(5).iter().all(|p| used.binary_search(p).is_ok()));
        assert!(normals(&done.mesh).iter().all(|n| n.z > 0.999));
        let area: f64 = (0..done.mesh.faces.len())
            .map(|f| {
                let [a, b, c] = skp_core::geometry::triangle_points(&done.mesh, f);
                area_vector(a, b, c).z / 2.0
            })
            .sum();
        assert!((area - 25.0).abs() < 1e-9);
        assert_partition(&mesh, &done);
    }

    #[test]
    fn a_cube_keeps_every_corner() {
        let points: Vec<Point> = (0..8)
            .map(|i| Point::new((i & 1) as f64, (i >> 1 & 1) as f64, (i >> 2) as f64))
            .collect();
        let triangles = [
            [0, 2, 3],
            [0, 3, 1],
            [4, 5, 7],
            [4, 7, 6],
            [0, 1, 5],
            [0, 5, 4],
            [2, 6, 7],
            [2, 7, 3],
            [0, 4, 6],
            [0, 6, 2],
            [1, 3, 7],
            [1, 7, 5],
        ];
        let mesh = build(&points, &triangles, &[FaceData::default(); 12]);
        let census = edge_counts(&mesh);
        assert_eq!((census.open, census.inconsistent), (0, 0));
        let done = run(&mesh, 4);
        assert_eq!(done.report.collapses, 0);
        assert_eq!(done.report.pinned_vertices, 8);
        assert_eq!(done.mesh.faces, mesh.faces);
        assert_partition(&mesh, &done);
    }

    #[test]
    fn a_material_seam_is_never_crossed() {
        let paint = |x: u32, _| FaceData {
            front: Some(MaterialId(u32::from(x >= 3))),
            ..FaceData::default()
        };
        let mut mesh = grid(6, paint);
        mesh.materials = vec![Default::default(), Default::default()];
        let done = run(&mesh, 1);
        assert!(done.report.collapses > 0);
        let used = used_positions(&done.mesh);
        assert!((0..=6).all(|y| used.binary_search(&(y * 7 + 3)).is_ok()));
        assert_partition(&mesh, &done);
    }

    #[test]
    fn a_uv_seam_is_never_crossed() {
        let mut mesh = grid(6, |_, _| FaceData::default());
        for f in 0..mesh.faces.len() {
            let corners = mesh.faces[f].corners().to_vec();
            let right = corners.iter().any(|&c| {
                mesh.positions[mesh.corners[c as usize].position as usize]
                    .x
                    .0
                    > 3.0
            });
            if right {
                for c in corners {
                    mesh.corners[c as usize].uvq.u += 100.0;
                }
            }
        }
        let done = run(&mesh, 1);
        assert!(done.report.collapses > 0);
        let used = used_positions(&done.mesh);
        assert!((0..=6).all(|y| used.binary_search(&(y * 7 + 3)).is_ok()));
        assert_partition(&mesh, &done);
    }

    #[test]
    fn a_sphere_reaches_its_target_without_folding() {
        let mesh = sphere(24, 12);
        let census = edge_counts(&mesh);
        assert_eq!(
            (census.open, census.inconsistent, census.non_manifold),
            (0, 0, 0)
        );
        let high = mesh.triangle_count();
        let target = high / 2;
        let done = run(&mesh, target);
        assert!(done.report.triangles <= target);
        let census = edge_counts(&done.mesh);
        assert_eq!(
            (census.open, census.inconsistent, census.non_manifold),
            (0, 0, 0)
        );
        for (f, n) in normals(&done.mesh).iter().enumerate() {
            let [a, b, c] = skp_core::geometry::triangle_points(&done.mesh, f);
            assert!(n.dot(a + b + c) > 0.0);
        }
        assert_partition(&mesh, &done);
        assert_eq!(done.mesh.faces, run(&mesh, target).mesh.faces);
    }

    #[test]
    fn a_mesh_at_or_under_target_is_left_alone() {
        let mesh = sphere(8, 4);
        let done = run(&mesh, mesh.triangle_count());
        assert_eq!(done.report.collapses, 0);
        assert_eq!(done.mesh.faces, mesh.faces);
        assert_partition(&mesh, &done);
    }

    #[test]
    fn a_zero_target_an_empty_mesh_or_a_cancel_is_refused() {
        let mesh = sphere(8, 4);
        let tolerance = skp_core::mesh::DEFAULT_WELD_TOLERANCE;
        let refuse = |mesh: &Mesh, target: usize, cancel: &CancelToken| {
            decimate(mesh, target, tolerance, cancel, &NoProgress).unwrap_err()
        };
        assert_eq!(
            refuse(&mesh, 0, &CancelToken::new()),
            RetopoError::ZeroTarget
        );
        assert_eq!(
            refuse(&Mesh::default(), 1, &CancelToken::new()),
            RetopoError::EmptyMesh
        );
        let cancel = CancelToken::new();
        cancel.cancel();
        assert_eq!(refuse(&mesh, 1, &cancel), RetopoError::Cancelled);
    }
}
