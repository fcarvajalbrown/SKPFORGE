use skp_core::correspondence::CorrespondenceError;
use skp_core::mesh::MeshError;
use skp_core::progress::Cancelled;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetopoError {
    EmptyMesh,
    ZeroTarget,
    Cancelled,
    InvalidMesh(MeshError),
    InvalidCorrespondence(CorrespondenceError),
}

impl fmt::Display for RetopoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RetopoError::EmptyMesh => write!(f, "the mesh has no triangles to route"),
            RetopoError::ZeroTarget => write!(f, "the target triangle count must be at least 1"),
            RetopoError::Cancelled => write!(f, "retopo cancelled by the caller"),
            RetopoError::InvalidMesh(e) => write!(f, "retopo was given an invalid mesh: {e}"),
            RetopoError::InvalidCorrespondence(e) => {
                write!(f, "retopo produced an invalid correspondence map: {e}")
            }
        }
    }
}

impl std::error::Error for RetopoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RetopoError::InvalidMesh(e) => Some(e),
            RetopoError::InvalidCorrespondence(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MeshError> for RetopoError {
    fn from(e: MeshError) -> Self {
        RetopoError::InvalidMesh(e)
    }
}

impl From<CorrespondenceError> for RetopoError {
    fn from(e: CorrespondenceError) -> Self {
        RetopoError::InvalidCorrespondence(e)
    }
}

impl From<Cancelled> for RetopoError {
    fn from(_: Cancelled) -> Self {
        RetopoError::Cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_without_debug_formatting() {
        assert_eq!(
            RetopoError::ZeroTarget.to_string(),
            "the target triangle count must be at least 1"
        );
        assert_eq!(
            RetopoError::InvalidMesh(MeshError::CornerOutOfRange { face: 1, corner: 9 })
                .to_string(),
            "retopo was given an invalid mesh: face 1 refers to corner 9, which does not exist"
        );
        assert_eq!(
            RetopoError::InvalidCorrespondence(CorrespondenceError::UnmappedLowTriangle {
                low: 3
            })
            .to_string(),
            "retopo produced an invalid correspondence map: low triangle 3 maps to no high triangle"
        );
    }
}
