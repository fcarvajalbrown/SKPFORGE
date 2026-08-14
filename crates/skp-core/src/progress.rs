use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        CancelToken(Arc::new(AtomicBool::new(false)))
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            return Err(Cancelled);
        }
        Ok(())
    }
}

impl Default for CancelToken {
    fn default() -> Self {
        CancelToken::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl fmt::Display for Cancelled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cancelled by the caller")
    }
}

impl std::error::Error for Cancelled {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Measured { done: u64, total: u64 },
    Unmeasured,
}

impl Progress {
    pub fn fraction(&self) -> Option<f64> {
        match self {
            Progress::Measured { done, total } => {
                if *total == 0 {
                    return None;
                }
                Some((*done as f64 / *total as f64).min(1.0))
            }
            Progress::Unmeasured => None,
        }
    }

    pub fn is_measured(&self) -> bool {
        matches!(self, Progress::Measured { .. })
    }
}

pub trait ProgressSink: Send + Sync {
    fn report(&self, progress: Progress);
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl ProgressSink for NoProgress {
    fn report(&self, _progress: Progress) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Progress>>);

    impl ProgressSink for Recorder {
        fn report(&self, progress: Progress) {
            self.0.lock().unwrap().push(progress);
        }
    }

    #[test]
    fn a_fresh_token_is_not_cancelled() {
        let token = CancelToken::new();
        assert!(!token.is_cancelled());
        assert_eq!(token.check(), Ok(()));
    }

    #[test]
    fn cancelling_one_handle_cancels_every_clone() {
        let token = CancelToken::new();
        let sidecar = token.clone();
        token.cancel();
        assert!(sidecar.is_cancelled());
        assert_eq!(sidecar.check(), Err(Cancelled));
    }

    #[test]
    fn a_cancelled_token_crosses_a_thread() {
        let token = CancelToken::new();
        let worker = token.clone();
        token.cancel();
        let seen = std::thread::spawn(move || worker.is_cancelled())
            .join()
            .unwrap();
        assert!(seen);
    }

    #[test]
    fn cancellation_displays_without_debug_formatting() {
        assert_eq!(Cancelled.to_string(), "cancelled by the caller");
    }

    #[test]
    fn a_measured_report_divides_into_a_fraction() {
        let progress = Progress::Measured { done: 1, total: 4 };
        assert_eq!(progress.fraction(), Some(0.25));
        assert!(progress.is_measured());
    }

    #[test]
    fn an_unmeasured_report_has_no_fraction() {
        let progress = Progress::Unmeasured;
        assert_eq!(progress.fraction(), None);
        assert!(!progress.is_measured());
    }

    #[test]
    fn a_measured_report_of_nothing_has_no_fraction() {
        let progress = Progress::Measured { done: 0, total: 0 };
        assert_eq!(progress.fraction(), None);
    }

    #[test]
    fn a_fraction_never_exceeds_one() {
        let progress = Progress::Measured { done: 9, total: 4 };
        assert_eq!(progress.fraction(), Some(1.0));
    }

    #[test]
    fn a_sink_receives_what_a_stage_reports() {
        let recorder = Recorder::default();
        recorder.report(Progress::Measured { done: 1, total: 2 });
        recorder.report(Progress::Unmeasured);
        assert_eq!(
            *recorder.0.lock().unwrap(),
            vec![
                Progress::Measured { done: 1, total: 2 },
                Progress::Unmeasured
            ]
        );
    }

    #[test]
    fn a_headless_run_can_discard_every_report() {
        let sink: &dyn ProgressSink = &NoProgress;
        sink.report(Progress::Unmeasured);
    }
}
