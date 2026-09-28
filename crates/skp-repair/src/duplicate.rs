use crate::compact::retain_faces;
use crate::geometry::triangle_positions;
use skp_core::mesh::Mesh;
use std::collections::HashSet;

fn rotated_to_lowest([a, b, c]: [u32; 3]) -> [u32; 3] {
    if a <= b && a <= c {
        [a, b, c]
    } else if b <= a && b <= c {
        [b, c, a]
    } else {
        [c, a, b]
    }
}

pub fn drop_duplicates(mesh: &mut Mesh) -> usize {
    let mut seen = HashSet::new();
    let keep: Vec<bool> = (0..mesh.faces.len())
        .map(|face| seen.insert(rotated_to_lowest(triangle_positions(mesh, face))))
        .collect();
    retain_faces(mesh, &keep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::indexed;
    use skp_core::mesh::{MaterialId, Point};

    fn points() -> Vec<Point> {
        vec![
            Point::new(0.0, 0.0, 0.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
        ]
    }

    #[test]
    fn the_same_triangle_from_another_start_corner_is_a_duplicate() {
        let mut mesh = indexed(&points(), &[[0, 1, 2], [1, 2, 0], [2, 0, 1]]);
        assert_eq!(drop_duplicates(&mut mesh), 2);
        assert_eq!(mesh.faces.len(), 1);
    }

    #[test]
    fn the_first_of_a_duplicate_set_is_the_one_kept() {
        let mut mesh = indexed(&points(), &[[0, 1, 2], [0, 1, 2]]);
        mesh.face_data[0].front = Some(MaterialId(3));
        drop_duplicates(&mut mesh);
        assert_eq!(mesh.face_data[0].front, Some(MaterialId(3)));
    }

    #[test]
    fn the_opposite_winding_is_not_a_duplicate() {
        let mut mesh = indexed(&points(), &[[0, 1, 2], [0, 2, 1]]);
        assert_eq!(drop_duplicates(&mut mesh), 0);
    }
}
