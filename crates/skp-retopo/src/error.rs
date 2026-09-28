use skp_core::mesh::MeshError;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetopoError {
    EmptyMesh,
    ZeroTarget,
    InvalidMesh(MeshError),
}

impl fmt::Display for RetopoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RetopoError::EmptyMesh => write!(f, "the mesh has no triangles to route"),
            RetopoError::ZeroTarget => write!(f, "the target triangle count must be at least 1"),
            RetopoError::InvalidMesh(e) => write!(f, "retopo was given an invalid mesh: {e}"),
        }
    }
}

impl std::error::Error for RetopoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RetopoError::InvalidMesh(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MeshError> for RetopoError {
    fn from(e: MeshError) -> Self {
        RetopoError::InvalidMesh(e)
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
    }
}
