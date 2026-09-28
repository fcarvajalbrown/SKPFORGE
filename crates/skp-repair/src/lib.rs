pub mod error;
pub mod report;

mod compact;
#[cfg(test)]
mod fixture;
mod weld;

use error::RepairError;
use report::RepairReport;
use skp_core::mesh::{Mesh, DEFAULT_WELD_TOLERANCE};
use skp_core::progress::{CancelToken, Progress, ProgressSink};
use skp_core::units::Uu;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RepairOptions {
    pub weld_tolerance: Uu,
}

impl Default for RepairOptions {
    fn default() -> Self {
        RepairOptions {
            weld_tolerance: DEFAULT_WELD_TOLERANCE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Repaired {
    pub mesh: Mesh,
    pub report: RepairReport,
}

const STAGES: u64 = 1;

struct Stages<'a> {
    done: u64,
    cancel: &'a CancelToken,
    progress: &'a dyn ProgressSink,
}

impl Stages<'_> {
    fn finished(&mut self) -> Result<(), RepairError> {
        self.done += 1;
        self.progress.report(Progress::Measured {
            done: self.done,
            total: STAGES,
        });
        self.cancel.check()?;
        Ok(())
    }
}

pub fn repair(
    input: &Mesh,
    options: &RepairOptions,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> Result<Repaired, RepairError> {
    input.validate()?;
    cancel.check()?;
    let mut stages = Stages {
        done: 0,
        cancel,
        progress,
    };
    let mut mesh = compact::triangulated(input);
    let mut report = RepairReport {
        positions_in: input.positions.len(),
        triangles_in: mesh.faces.len(),
        ..RepairReport::default()
    };

    report.positions_welded = weld::weld(&mut mesh, options.weld_tolerance);
    stages.finished()?;

    compact::compact(&mut mesh);
    report.positions_out = mesh.positions.len();
    report.triangles_out = mesh.faces.len();
    mesh.validate()?;
    Ok(Repaired { mesh, report })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::soup;
    use skp_core::mesh::Point;
    use skp_core::progress::NoProgress;
    use std::sync::Mutex;

    fn two_triangle_soup() -> Mesh {
        soup(&[
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(1.0, 1.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
        ])
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Progress>>);

    impl ProgressSink for Recorder {
        fn report(&self, progress: Progress) {
            self.0.lock().unwrap().push(progress);
        }
    }

    #[test]
    fn repair_welds_and_reports_counts() {
        let repaired = repair(
            &two_triangle_soup(),
            &RepairOptions::default(),
            &CancelToken::new(),
            &NoProgress,
        )
        .unwrap();
        assert_eq!(repaired.report.positions_in, 6);
        assert_eq!(repaired.report.positions_welded, 2);
        assert_eq!(repaired.report.positions_out, 4);
        assert_eq!(repaired.report.triangles_out, 2);
        assert_eq!(repaired.mesh.validate(), Ok(()));
    }

    #[test]
    fn a_wider_tolerance_welds_more() {
        let options = RepairOptions {
            weld_tolerance: Uu(1.5),
        };
        let repaired = repair(
            &two_triangle_soup(),
            &options,
            &CancelToken::new(),
            &NoProgress,
        )
        .unwrap();
        assert!(repaired.report.positions_out < 4);
    }

    #[test]
    fn a_cancelled_run_stops_before_any_work() {
        let cancel = CancelToken::new();
        cancel.cancel();
        let result = repair(
            &two_triangle_soup(),
            &RepairOptions::default(),
            &cancel,
            &NoProgress,
        );
        assert_eq!(result.unwrap_err(), RepairError::Cancelled);
    }

    #[test]
    fn progress_is_measured_in_stages_and_ends_complete() {
        let recorder = Recorder::default();
        repair(
            &two_triangle_soup(),
            &RepairOptions::default(),
            &CancelToken::new(),
            &recorder,
        )
        .unwrap();
        let reports = recorder.0.lock().unwrap();
        assert_eq!(reports.last().and_then(|p| p.fraction()), Some(1.0));
        assert!(reports.iter().all(|p| p.is_measured()));
    }

    #[test]
    fn an_invalid_input_is_refused() {
        let mut mesh = two_triangle_soup();
        mesh.face_data.pop();
        let result = repair(
            &mesh,
            &RepairOptions::default(),
            &CancelToken::new(),
            &NoProgress,
        );
        assert!(matches!(result, Err(RepairError::InvalidMesh(_))));
    }
}
