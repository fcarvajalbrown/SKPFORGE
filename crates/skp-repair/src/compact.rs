use skp_core::mesh::{Face, Mesh};

pub fn triangulated(mesh: &Mesh) -> Mesh {
    let mut faces = Vec::with_capacity(mesh.triangle_count());
    let mut face_data = Vec::with_capacity(mesh.triangle_count());
    for (face, data) in mesh.faces.iter().zip(&mesh.face_data) {
        for tri in face.triangulate() {
            faces.push(Face::Tri(tri));
            face_data.push(*data);
        }
    }
    Mesh {
        faces,
        face_data,
        ..mesh.clone()
    }
}

pub fn compact(mesh: &mut Mesh) {
    let mut corner_map = vec![u32::MAX; mesh.corners.len()];
    let mut corners = Vec::new();
    for face in &mut mesh.faces {
        let face_corners: &mut [u32] = match face {
            Face::Tri(c) => c,
            Face::Quad(c) => c,
        };
        for corner in face_corners {
            let slot = &mut corner_map[*corner as usize];
            if *slot == u32::MAX {
                *slot = corners.len() as u32;
                corners.push(mesh.corners[*corner as usize]);
            }
            *corner = *slot;
        }
    }
    let mut position_map = vec![u32::MAX; mesh.positions.len()];
    let mut positions = Vec::new();
    for corner in &mut corners {
        let slot = &mut position_map[corner.position as usize];
        if *slot == u32::MAX {
            *slot = positions.len() as u32;
            positions.push(mesh.positions[corner.position as usize]);
        }
        corner.position = *slot;
    }
    mesh.corners = corners;
    mesh.positions = positions;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::{indexed, position_triangles};
    use skp_core::mesh::{Corner, FaceData, MaterialId, Point};

    #[test]
    fn a_quad_becomes_two_triangles_with_its_face_data() {
        let mesh = Mesh {
            positions: vec![Point::new(0.0, 0.0, 0.0); 4],
            corners: (0..4)
                .map(|position| Corner {
                    position,
                    ..Corner::default()
                })
                .collect(),
            faces: vec![Face::Quad([0, 1, 2, 3])],
            face_data: vec![FaceData {
                front: Some(MaterialId(0)),
                ..FaceData::default()
            }],
            materials: Vec::new(),
        };
        let tris = triangulated(&mesh);
        assert_eq!(tris.faces, vec![Face::Tri([0, 1, 2]), Face::Tri([0, 2, 3])]);
        assert_eq!(tris.face_data, vec![mesh.face_data[0]; 2]);
    }

    #[test]
    fn compacting_drops_what_no_face_uses_and_keeps_geometry() {
        let points = [
            Point::new(0.0, 0.0, 0.0),
            Point::new(9.0, 9.0, 9.0),
            Point::new(1.0, 0.0, 0.0),
            Point::new(0.0, 1.0, 0.0),
        ];
        let mut mesh = indexed(&points, &[[0, 1, 2], [0, 2, 3]]);
        mesh.faces.remove(0);
        mesh.face_data.remove(0);
        compact(&mut mesh);
        assert_eq!(mesh.corners.len(), 3);
        assert_eq!(mesh.positions.len(), 3);
        let tri = position_triangles(&mesh)[0];
        assert_eq!(
            tri.map(|p| mesh.positions[p as usize]),
            [points[0], points[2], points[3]]
        );
        assert_eq!(mesh.validate(), Ok(()));
    }
}
