pub mod error;
pub mod flatten;
pub mod report;
pub mod scene;
#[cfg(feature = "sdk")]
mod sdk;

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
    #[cfg(feature = "sdk")]
    {
        let scene = sdk::reader::read_scene(path, cancel)?;
        let (mesh, report) = flatten::flatten(&scene, cancel, progress)?;
        Ok(Import { mesh, report })
    }
    #[cfg(not(feature = "sdk"))]
    {
        let _ = (path, cancel, progress);
        Err(IoError::SdkUnavailable)
    }
}

#[cfg(all(test, not(feature = "sdk")))]
mod tests {
    use super::*;
    use skp_core::progress::NoProgress;

    #[test]
    fn stub_build_refuses_to_import() {
        let result = import(Path::new("model.skp"), &CancelToken::new(), &NoProgress);
        assert_eq!(result.unwrap_err(), IoError::SdkUnavailable);
    }
}
