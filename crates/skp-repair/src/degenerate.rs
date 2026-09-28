use crate::compact::retain_faces;
use crate::geometry::{height, triangle_points, triangle_positions};
use skp_core::mesh::Mesh;
use skp_core::units::Uu;

pub fn is_degenerate(mesh: &Mesh, face: usize, tolerance: Uu) -> bool {
    let [a, b, c] = triangle_positions(mesh, face);
    if a == b || b == c || c == a {
        return true;
    }
    let [pa, pb, pc] = triangle_points(mesh, face);
    height(pa, pb, pc) < tolerance.0
}

pub fn drop_degenerates(mesh: &mut Mesh, tolerance: Uu) -> usize {
    let keep: Vec<bool> = (0..mesh.faces.len())
        .map(|face| !is_degenerate(mesh, face, tolerance))
        .collect();
    retain_faces(mesh, &keep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::indexed;
    use skp_core::mesh::{Point, DEFAULT_WELD_TOLERANCE};

    fn points() -> Vec<Point> {
        vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(10.0, 0.0, 0.0),
            Point::new(0.0, 10.0, 0.0),
            Point::new(5.0, 0.0, 0.0),
            Point::new(5.0, 0.001, 0.0),
            Point::new(5.0, 0.01, 0.0),
        ]
    }

    #[test]
    fn a_triangle_with_a_repeated_position_is_dropped() {
        let mut mesh = indexed(&points(), &[[0, 1, 2], [0, 1, 1]]);
        assert_eq!(drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE), 1);
        assert_eq!(mesh.faces.len(), 1);
    }

    #[test]
    fn a_collinear_triangle_is_dropped() {
        let mut mesh = indexed(&points(), &[[0, 1, 3], [0, 1, 2]]);
        assert_eq!(drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE), 1);
        assert_eq!(mesh.face_data.len(), 1);
    }

    #[test]
    fn a_sliver_thinner_than_the_tolerance_is_dropped() {
        let mut mesh = indexed(&points(), &[[0, 1, 4]]);
        assert_eq!(drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE), 1);
    }

    #[test]
    fn a_thin_but_real_triangle_is_kept() {
        let mut mesh = indexed(&points(), &[[0, 1, 5]]);
        assert_eq!(drop_degenerates(&mut mesh, DEFAULT_WELD_TOLERANCE), 0);
    }
}
