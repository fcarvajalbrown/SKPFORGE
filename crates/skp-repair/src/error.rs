use skp_core::mesh::MeshError;
use skp_core::progress::Cancelled;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairError {
    Cancelled,
    InvalidMesh(MeshError),
}

impl fmt::Display for RepairError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepairError::Cancelled => write!(f, "repair cancelled by the caller"),
            RepairError::InvalidMesh(e) => write!(f, "repair produced an invalid mesh: {e}"),
        }
    }
}

impl std::error::Error for RepairError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepairError::Cancelled => None,
            RepairError::InvalidMesh(e) => Some(e),
        }
    }
}

impl From<Cancelled> for RepairError {
    fn from(_: Cancelled) -> Self {
        RepairError::Cancelled
    }
}

impl From<MeshError> for RepairError {
    fn from(e: MeshError) -> Self {
        RepairError::InvalidMesh(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_display_without_debug_formatting() {
        assert_eq!(
            RepairError::Cancelled.to_string(),
            "repair cancelled by the caller"
        );
        assert_eq!(
            RepairError::InvalidMesh(MeshError::CornerOutOfRange { face: 1, corner: 9 })
                .to_string(),
            "repair produced an invalid mesh: face 1 refers to corner 9, which does not exist"
        );
    }
}
