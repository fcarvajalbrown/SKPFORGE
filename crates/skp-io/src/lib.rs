pub mod error;
pub mod flatten;
pub mod report;
pub mod scene;

use error::IoError;
use report::ImportReport;
use skp_core::mesh::Mesh;
use skp_core::progress::{CancelToken, ProgressSink};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Import {
    pub mesh: Mesh,
    pub report: ImportReport,
}

pub fn import(
    path: &Path,
    cancel: &CancelToken,
    progress: &dyn ProgressSink,
) -> Result<Import, IoError> {
    let _ = (path, cancel, progress);
    Err(IoError::SdkUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skp_core::progress::NoProgress;

    #[test]
    fn stub_build_refuses_to_import() {
        let result = import(Path::new("model.skp"), &CancelToken::new(), &NoProgress);
        assert_eq!(result.unwrap_err(), IoError::SdkUnavailable);
    }
}
