use skp_core::mesh::{Mesh, Point};
use skp_core::units::Uu;

pub fn weld(mesh: &mut Mesh, tolerance: Uu) -> usize {
    let remap = mesh.weld_map(tolerance);
    let mut positions: Vec<Point> = Vec::new();
    for (source, &target) in remap.iter().enumerate() {
        if target as usize == positions.len() {
            positions.push(mesh.positions[source]);
        }
    }
    for corner in &mut mesh.corners {
        corner.position = remap[corner.position as usize];
    }
    let merged = mesh.positions.len() - positions.len();
    mesh.positions = positions;
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::soup;
    use skp_core::mesh::DEFAULT_WELD_TOLERANCE;

    #[test]
    fn two_triangles_sharing_an_edge_end_up_sharing_its_positions() {
        let mut mesh = soup(&[
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(1.0, 1.0, 0.0),
            Point::new(0.0, 1.0 + 0.001, 0.0),
        ]);
        let merged = weld(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!(merged, 2);
        assert_eq!(mesh.positions.len(), 4);
        assert_eq!(mesh.corners[1].position, mesh.corners[3].position);
        assert_eq!(mesh.corners[2].position, mesh.corners[5].position);
        assert_eq!(mesh.validate(), Ok(()));
    }

    #[test]
    fn a_welded_position_keeps_its_first_occurrence() {
        let mut mesh = soup(&[
            Point::new(0.0, 1.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 0.0, 0.0),
            Point::new(0.0, 1.0 + 0.001, 0.0),
            Point::new(1.0, 1.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
        ]);
        weld(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!(mesh.positions[0], Point::new(0.0, 1.0, 0.0));
    }

    #[test]
    fn corners_keep_their_own_attributes_when_positions_merge() {
        let mut mesh = soup(&[
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
            Point::new(0.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
            Point::new(-1.0, 0.0, 0.0),
        ]);
        mesh.corners[3].uvq.u = 7.0;
        weld(&mut mesh, DEFAULT_WELD_TOLERANCE);
        assert_eq!(mesh.corners.len(), 6);
        assert_eq!(mesh.corners[0].position, mesh.corners[3].position);
        assert_eq!(mesh.corners[0].uvq.u, 0.0);
        assert_eq!(mesh.corners[3].uvq.u, 7.0);
    }
}
