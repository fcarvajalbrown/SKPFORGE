use crate::error::RetopoError;
use skp_core::geometry::{area_vector, Vec3};
use skp_core::mesh::Mesh;
use skp_core::topology::{Edges, Incidence};
use std::fmt;

pub const SHARP_ANGLE_DEGREES: f64 = 30.0;
pub const NEAR_TARGET_RATIO: f64 = 1.5;
pub const HEAVY_RATIO: f64 = 3.0;
pub const CAD_SHARP_FRACTION: f64 = 0.35;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RouteChoice {
    #[default]
    Auto,
    A,
    B,
}

impl RouteChoice {
    pub fn from_flag(value: &str) -> Option<RouteChoice> {
        match value {
            "auto" => Some(RouteChoice::Auto),
            "a" => Some(RouteChoice::A),
            "b" => Some(RouteChoice::B),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RouteOptions {
    pub target_triangles: Option<usize>,
    pub choice: RouteChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    A,
    B,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    NearTarget,
    CadLike,
    Organic,
    OverTarget,
    Forced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub route: Route,
    pub reason: Reason,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RouteMetrics {
    pub input_triangles: usize,
    pub target_triangles: usize,
    pub ratio: f64,
    pub sharp: f64,
    pub sharp_edges: usize,
    pub measured_edges: usize,
    pub open_edges: usize,
    pub unmeasured_edges: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Routed {
    pub metrics: RouteMetrics,
    pub decision: Decision,
}

pub fn route(mesh: &Mesh, options: &RouteOptions) -> Result<Routed, RetopoError> {
    let metrics = measure(mesh, options.target_triangles)?;
    Ok(Routed {
        metrics,
        decision: decide(&metrics, options.choice),
    })
}

pub fn measure(mesh: &Mesh, target: Option<usize>) -> Result<RouteMetrics, RetopoError> {
    mesh.validate()?;
    let triangles = position_triangles(mesh);
    if triangles.is_empty() {
        return Err(RetopoError::EmptyMesh);
    }
    let target_triangles = target.unwrap_or(triangles.len());
    if target_triangles == 0 {
        return Err(RetopoError::ZeroTarget);
    }
    let normals: Vec<Option<Vec3>> = triangles
        .iter()
        .map(|&[a, b, c]| {
            let [a, b, c] = [a, b, c].map(|p| Vec3::of(mesh.positions[p as usize]));
            area_vector(a, b, c).normalised()
        })
        .collect();
    let mut metrics = RouteMetrics {
        input_triangles: triangles.len(),
        target_triangles,
        ratio: triangles.len() as f64 / target_triangles as f64,
        ..RouteMetrics::default()
    };
    let limit = SHARP_ANGLE_DEGREES.to_radians().cos();
    for (_, incidences) in Edges::build(&triangles).iter() {
        if incidences.len() < 2 {
            metrics.open_edges += 1;
            continue;
        }
        match smallest_cosine(incidences, &normals) {
            None => metrics.unmeasured_edges += 1,
            Some(cosine) => {
                metrics.measured_edges += 1;
                if cosine < limit {
                    metrics.sharp_edges += 1;
                }
            }
        }
    }
    if metrics.measured_edges > 0 {
        metrics.sharp = metrics.sharp_edges as f64 / metrics.measured_edges as f64;
    }
    Ok(metrics)
}

pub fn decide(metrics: &RouteMetrics, choice: RouteChoice) -> Decision {
    let forced = |route| Decision {
        route,
        reason: Reason::Forced,
    };
    match choice {
        RouteChoice::A => return forced(Route::A),
        RouteChoice::B => return forced(Route::B),
        RouteChoice::Auto => {}
    }
    let (route, reason) = if metrics.ratio <= NEAR_TARGET_RATIO {
        (Route::A, Reason::NearTarget)
    } else if metrics.sharp >= CAD_SHARP_FRACTION {
        (Route::A, Reason::CadLike)
    } else if metrics.ratio > HEAVY_RATIO {
        (Route::B, Reason::Organic)
    } else {
        (Route::A, Reason::OverTarget)
    };
    Decision { route, reason }
}

fn position_triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
    mesh.faces
        .iter()
        .flat_map(|f| f.triangulate())
        .map(|t| t.map(|corner| mesh.corners[corner as usize].position))
        .collect()
}

fn smallest_cosine(incidences: &[Incidence], normals: &[Option<Vec3>]) -> Option<f64> {
    if let [a, b] = incidences {
        let na = normals[a.face as usize]?;
        let nb = normals[b.face as usize]?;
        let facing = if a.forward == b.forward { -1.0 } else { 1.0 };
        return Some(facing * na.dot(nb));
    }
    let present: Vec<Vec3> = incidences
        .iter()
        .filter_map(|i| normals[i.face as usize])
        .collect();
    let mut smallest: Option<f64> = None;
    for (i, a) in present.iter().enumerate() {
        for b in &present[i + 1..] {
            let cosine = a.dot(*b);
            smallest = Some(smallest.map_or(cosine, |s: f64| s.min(cosine)));
        }
    }
    smallest
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Route::A => write!(f, "A (tri-to-quad pairing)"),
            Route::B => write!(f, "B (field-aligned remesh)"),
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Reason::NearTarget => "already near target, ratio <= 1.5",
            Reason::CadLike => "CAD-like, sharp >= 0.35, corners must survive",
            Reason::Organic => "organic and heavy, ratio > 3.0 and sharp < 0.35",
            Reason::OverTarget => "over target, decimation needed",
            Reason::Forced => "forced by --route",
        };
        write!(f, "{text}")
    }
}

impl fmt::Display for Routed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = &self.metrics;
        writeln!(f, "input triangles        {}", m.input_triangles)?;
        writeln!(f, "target triangles       {}", m.target_triangles)?;
        writeln!(f, "ratio                  {:.3}", m.ratio)?;
        writeln!(
            f,
            "sharp                  {:.3} ({} of {} measured edges over {} degrees)",
            m.sharp, m.sharp_edges, m.measured_edges, SHARP_ANGLE_DEGREES
        )?;
        writeln!(
            f,
            "left out of sharp      {} open edges, {} with no usable normal",
            m.open_edges, m.unmeasured_edges
        )?;
        write!(
            f,
            "route                  {}, {}",
            self.decision.route, self.decision.reason
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skp_core::mesh::{Corner, Face, FaceData, Point};

    fn indexed(points: &[Point], triangles: &[[u32; 3]]) -> Mesh {
        let mut mesh = Mesh {
            positions: points.to_vec(),
            ..Mesh::default()
        };
        for tri in triangles {
            let first = mesh.corners.len() as u32;
            for &position in tri {
                mesh.corners.push(Corner {
                    position,
                    ..Corner::default()
                });
            }
            mesh.faces.push(Face::Tri([first, first + 1, first + 2]));
            mesh.face_data.push(FaceData::default());
        }
        mesh
    }

    fn cube() -> Mesh {
        let p = |x, y, z| Point::new(x, y, z);
        indexed(
            &[
                p(0.0, 0.0, 0.0),
                p(1.0, 0.0, 0.0),
                p(1.0, 1.0, 0.0),
                p(0.0, 1.0, 0.0),
                p(0.0, 0.0, 1.0),
                p(1.0, 0.0, 1.0),
                p(1.0, 1.0, 1.0),
                p(0.0, 1.0, 1.0),
            ],
            &[
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
            ],
        )
    }

    fn hinge(degrees: f64) -> Mesh {
        let (s, c) = degrees.to_radians().sin_cos();
        indexed(
            &[
                Point::new(0.0, 0.0, 0.0),
                Point::new(0.0, 1.0, 0.0),
                Point::new(-1.0, 0.5, 0.0),
                Point::new(c, 0.5, s),
            ],
            &[[0, 1, 2], [1, 0, 3]],
        )
    }

    fn metrics(mesh: &Mesh) -> RouteMetrics {
        measure(mesh, None).unwrap()
    }

    #[test]
    fn a_cube_has_twelve_sharp_edges_and_six_flat_diagonals() {
        let m = metrics(&cube());
        assert_eq!(m.measured_edges, 18);
        assert_eq!(m.sharp_edges, 12);
        assert_eq!(m.open_edges, 0);
        assert!((m.sharp - 12.0 / 18.0).abs() < 1e-12);
    }

    #[test]
    fn the_sharp_threshold_sits_at_thirty_degrees() {
        assert_eq!(metrics(&hinge(0.0)).sharp_edges, 0);
        assert_eq!(metrics(&hinge(29.0)).sharp_edges, 0);
        assert_eq!(metrics(&hinge(31.0)).sharp_edges, 1);
        assert_eq!(metrics(&hinge(90.0)).sharp_edges, 1);
    }

    #[test]
    fn open_edges_are_left_out_of_the_fraction() {
        let m = metrics(&hinge(0.0));
        assert_eq!(m.open_edges, 4);
        assert_eq!(m.measured_edges, 1);
        assert_eq!(m.sharp, 0.0);
    }

    #[test]
    fn an_inconsistently_wound_flat_edge_is_not_sharp() {
        let flat = indexed(
            &[
                Point::new(0.0, 0.0, 0.0),
                Point::new(0.0, 1.0, 0.0),
                Point::new(-1.0, 0.5, 0.0),
                Point::new(1.0, 0.5, 0.0),
            ],
            &[[0, 1, 2], [0, 1, 3]],
        );
        assert_eq!(metrics(&flat).sharp_edges, 0);
    }

    #[test]
    fn a_wall_t_junction_counts_as_sharp() {
        let t = indexed(
            &[
                Point::new(0.0, 0.0, 0.0),
                Point::new(0.0, 0.0, 1.0),
                Point::new(-1.0, 0.0, 0.5),
                Point::new(1.0, 0.0, 0.5),
                Point::new(0.0, 1.0, 0.5),
            ],
            &[[0, 1, 2], [1, 0, 3], [0, 1, 4]],
        );
        let m = metrics(&t);
        assert_eq!(m.sharp_edges, 1);
        assert_eq!(m.measured_edges, 1);
    }

    #[test]
    fn the_target_defaults_to_the_input_count() {
        let m = metrics(&cube());
        assert_eq!(m.target_triangles, 12);
        assert_eq!(m.ratio, 1.0);
        let over = measure(&cube(), Some(4)).unwrap();
        assert_eq!(over.ratio, 3.0);
    }

    #[test]
    fn an_empty_mesh_or_a_zero_target_is_refused() {
        assert_eq!(measure(&Mesh::new(), None), Err(RetopoError::EmptyMesh));
        assert_eq!(measure(&cube(), Some(0)), Err(RetopoError::ZeroTarget));
    }

    fn with(ratio: f64, sharp: f64) -> RouteMetrics {
        RouteMetrics {
            ratio,
            sharp,
            ..RouteMetrics::default()
        }
    }

    fn auto(ratio: f64, sharp: f64) -> Decision {
        decide(&with(ratio, sharp), RouteChoice::Auto)
    }

    #[test]
    fn the_router_follows_the_prd_table_at_its_boundaries() {
        let a = |reason| Decision {
            route: Route::A,
            reason,
        };
        assert_eq!(auto(1.5, 0.0), a(Reason::NearTarget));
        assert_eq!(auto(1.5, 0.9), a(Reason::NearTarget));
        assert_eq!(auto(10.0, 0.35), a(Reason::CadLike));
        assert_eq!(auto(3.0, 0.1), a(Reason::OverTarget));
        assert_eq!(auto(1.51, 0.34), a(Reason::OverTarget));
        assert_eq!(
            auto(3.01, 0.34),
            Decision {
                route: Route::B,
                reason: Reason::Organic
            }
        );
    }

    #[test]
    fn a_forced_route_overrides_the_table() {
        let m = with(10.0, 0.0);
        assert_eq!(decide(&m, RouteChoice::A).route, Route::A);
        assert_eq!(decide(&m, RouteChoice::A).reason, Reason::Forced);
        assert_eq!(decide(&with(1.0, 0.9), RouteChoice::B).route, Route::B);
    }

    #[test]
    fn route_flags_parse_in_lower_case_only() {
        assert_eq!(RouteChoice::from_flag("auto"), Some(RouteChoice::Auto));
        assert_eq!(RouteChoice::from_flag("a"), Some(RouteChoice::A));
        assert_eq!(RouteChoice::from_flag("b"), Some(RouteChoice::B));
        assert_eq!(RouteChoice::from_flag("c"), None);
    }

    #[test]
    fn route_measures_and_decides_in_one_call() {
        let routed = route(&cube(), &RouteOptions::default()).unwrap();
        assert_eq!(routed.decision.reason, Reason::NearTarget);
        assert!(routed.to_string().contains("route                  A"));
    }
}
