use skp_core::mesh::MeshError;
use skp_core::progress::Cancelled;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoError {
    SdkUnavailable,
    PathNotUtf8,
    Sdk { call: &'static str, code: i32 },
    DegenerateTransform { face: usize },
    Cancelled,
    Mesh(MeshError),
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::SdkUnavailable => write!(
                f,
                "skp-io was built without the sdk feature, so .skp files cannot be read"
            ),
            IoError::PathNotUtf8 => write!(f, "the model path is not valid UTF-8"),
            IoError::Sdk { call, code } => {
                write!(f, "{call} failed with SketchUp error code {code}")
            }
            IoError::DegenerateTransform { face } => write!(
                f,
                "face {face} sits under a transform that maps it to infinity"
            ),
            IoError::Cancelled => write!(f, "import cancelled by the caller"),
            IoError::Mesh(e) => write!(f, "imported mesh is malformed: {e}"),
        }
    }
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IoError::Mesh(e) => Some(e),
            _ => None,
        }
    }
}

impl From<Cancelled> for IoError {
    fn from(_: Cancelled) -> Self {
        IoError::Cancelled
    }
}

impl From<MeshError> for IoError {
    fn from(e: MeshError) -> Self {
        IoError::Mesh(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_without_debug_formatting() {
        assert_eq!(
            IoError::Sdk {
                call: "SUModelCreateFromFileWithStatus",
                code: 12
            }
            .to_string(),
            "SUModelCreateFromFileWithStatus failed with SketchUp error code 12"
        );
        assert_eq!(
            IoError::DegenerateTransform { face: 3 }.to_string(),
            "face 3 sits under a transform that maps it to infinity"
        );
    }
}
