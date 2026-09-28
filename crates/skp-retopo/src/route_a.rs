use crate::decimate::{decimate, DecimateReport};
use crate::error::RetopoError;
use crate::pair::{pair, PairReport};
use skp_core::correspondence::{Correspondence, CorrespondenceBuilder};
use skp_core::mesh::Mesh;
use skp_core::progress::{CancelToken, ProgressSink};
use skp_core::units::Uu;
use std::fmt;

#[derive(Debug, Clone)]
pub struct RoutedA {
    pub mesh: Mesh,
    pub correspondence: Correspondence,
    pub target_triangles: usize,
    pub decimated: Option<DecimateReport>,
    pub paired: PairReport,
}

pub fn route_a(
    mesh: &Mesh,
    target: usize,
    tolerance: Uu,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> Result<RoutedA, RetopoError> {
    if mesh.triangle_count() <= target {
        let paired = pair(mesh, tolerance, cancel, progress)?;
        return Ok(RoutedA {
            mesh: paired.mesh,
            correspondence: paired.correspondence,
            target_triangles: target,
            decimated: None,
            paired: paired.report,
        });
    }
    let decimated = decimate(mesh, target, tolerance, cancel, progress)?;
    let paired = pair(&decimated.mesh, tolerance, cancel, progress)?;
    let correspondence = compose(&paired.correspondence, &decimated.correspondence);
    correspondence.validate()?;
    Ok(RoutedA {
        mesh: paired.mesh,
        correspondence,
        target_triangles: target,
        decimated: Some(decimated.report),
        paired: paired.report,
    })
}

fn compose(outer: &Correspondence, inner: &Correspondence) -> Correspondence {
    let count = outer.low_triangle_count();
    let mut builder = CorrespondenceBuilder::new(count);
    for low in 0..count as u32 {
        for &middle in outer.high_for(low) {
            for &high in inner.high_for(middle) {
                builder.push(low, high);
            }
        }
    }
    builder.build()
}

impl RoutedA {
    pub fn over_budget(&self) -> usize {
        self.mesh
            .triangle_count()
            .saturating_sub(self.target_triangles)
    }
}

impl fmt::Display for RoutedA {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(report) = &self.decimated {
            writeln!(f, "{report}")?;
        }
        writeln!(f, "{}", self.paired)?;
        match self.over_budget() {
            0 => write!(f, "budget                 met"),
            over => write!(
                f,
                "budget                 missed by {over} triangles; no further collapse keeps every locked edge"
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skp_core::mesh::{Corner, Face, FaceData, Normal, Point, Uvq};
    use skp_core::progress::NoProgress;

    fn dome(segments: u32, rings: u32) -> Mesh {
        let mut mesh = Mesh {
            positions: vec![Point::new(0.0, 0.0, 10.0)],
            ..Mesh::default()
        };
        for i in 1..=rings {
            let polar = std::f64::consts::FRAC_PI_2 * i as f64 / rings as f64;
            for j in 0..segments {
                let around = std::f64::consts::TAU * j as f64 / segments as f64;
                mesh.positions.push(Point::new(
                    10.0 * polar.sin() * around.cos(),
                    10.0 * polar.sin() * around.sin(),
                    10.0 * polar.cos(),
                ));
            }
        }
        let at = |i: u32, j: u32| 1 + (i - 1) * segments + j % segments;
        let mut triangles = Vec::new();
        for j in 0..segments {
            triangles.push([0, at(1, j), at(1, j + 1)]);
            for i in 1..rings {
                let (a, b) = (at(i, j), at(i, j + 1));
                let (c, d) = (at(i + 1, j), at(i + 1, j + 1));
                triangles.push([a, c, d]);
                triangles.push([a, d, b]);
            }
        }
        for tri in triangles {
            let first = mesh.corners.len() as u32;
            for position in tri {
                mesh.corners.push(Corner {
                    position,
                    uvq: Uvq::default(),
                    back_uvq: Uvq::default(),
                    normal: Normal::default(),
                });
            }
            mesh.faces.push(Face::Tri([first, first + 1, first + 2]));
            mesh.face_data.push(FaceData::default());
        }
        mesh
    }

    fn run(mesh: &Mesh, target: usize) -> RoutedA {
        route_a(
            mesh,
            target,
            skp_core::mesh::DEFAULT_WELD_TOLERANCE,
            &CancelToken::new(),
            &NoProgress,
        )
        .unwrap()
    }

    fn assert_covers_each_high_once(mesh: &Mesh, done: &RoutedA) {
        let mut seen = vec![0usize; mesh.triangle_count()];
        for low in 0..done.mesh.triangle_count() as u32 {
            for &high in done.correspondence.high_for(low) {
                seen[high as usize] += 1;
            }
        }
        assert!(seen.iter().all(|&n| n == 1), "{seen:?}");
    }

    #[test]
    fn under_budget_only_pairs() {
        let mesh = dome(16, 6);
        let done = run(&mesh, mesh.triangle_count());
        assert_eq!(done.decimated, None);
        assert_eq!(done.over_budget(), 0);
        assert_eq!(done.mesh.triangle_count(), mesh.triangle_count());
        assert_covers_each_high_once(&mesh, &done);
    }

    #[test]
    fn over_budget_decimates_then_pairs_and_maps_back_to_the_input() {
        let mesh = dome(16, 6);
        let target = mesh.triangle_count() / 2;
        let done = run(&mesh, target);
        let report = done.decimated.unwrap();
        assert!(report.collapses > 0);
        assert!(done.mesh.triangle_count() <= target);
        assert_eq!(done.over_budget(), 0);
        assert_eq!(done.mesh.triangle_count(), report.triangles);
        assert_eq!(done.correspondence.low_triangle_count(), report.triangles);
        assert_covers_each_high_once(&mesh, &done);
    }

    #[test]
    fn a_budget_that_would_cost_a_corner_is_missed_and_reported() {
        let mut mesh = dome(4, 1);
        for p in &mut mesh.positions[1..] {
            p.z = skp_core::units::Uu(0.0);
        }
        let done = run(&mesh, 1);
        assert_eq!(done.decimated.unwrap().collapses, 0);
        assert_eq!(done.over_budget(), mesh.triangle_count() - 1);
        assert!(done
            .to_string()
            .ends_with("missed by 3 triangles; no further collapse keeps every locked edge"));
    }
}
