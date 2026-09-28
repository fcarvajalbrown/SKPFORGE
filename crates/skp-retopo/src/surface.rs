use skp_core::geometry::{area_vector, Vec3};
use skp_core::mesh::{Corner, FaceData, Mesh};
use skp_core::topology::{Edges, Incidence};

pub struct Surface<'a> {
    mesh: &'a Mesh,
    pub triangles: Vec<[u32; 3]>,
    pub normals: Vec<Option<Vec3>>,
    pub edges: Edges,
}

impl<'a> Surface<'a> {
    pub fn of(mesh: &'a Mesh) -> Surface<'a> {
        let triangles: Vec<[u32; 3]> = mesh
            .faces
            .iter()
            .flat_map(|f| f.triangulate())
            .map(|t| t.map(|corner| mesh.corners[corner as usize].position))
            .collect();
        let normals = triangles
            .iter()
            .map(|&[a, b, c]| {
                let [a, b, c] = [a, b, c].map(|p| Vec3::of(mesh.positions[p as usize]));
                area_vector(a, b, c).normalised()
            })
            .collect();
        let edges = Edges::build(&triangles);
        Surface {
            mesh,
            triangles,
            normals,
            edges,
        }
    }

    pub fn point(&self, p: u32) -> Vec3 {
        Vec3::of(self.mesh.positions[p as usize])
    }

    pub fn far_vertex(&self, face: u32, (from, to): (u32, u32)) -> Option<u32> {
        self.triangles[face as usize]
            .into_iter()
            .find(|&p| p != from && p != to)
    }

    pub fn is_coplanar(&self, edge: (u32, u32), incidences: &[Incidence], tolerance: f64) -> bool {
        let [a, b] = incidences else {
            return false;
        };
        let (Some(na), Some(nb)) = (self.normals[a.face as usize], self.normals[b.face as usize])
        else {
            return false;
        };
        let facing = if a.forward == b.forward { -1.0 } else { 1.0 };
        if facing * na.dot(nb) <= 0.0 {
            return false;
        }
        let offset = |face: u32| {
            self.far_vertex(face, edge)
                .map(|p| self.point(p) - self.point(edge.0))
        };
        let (Some(to_a), Some(to_b)) = (offset(a.face), offset(b.face)) else {
            return false;
        };
        na.dot(to_b).abs() <= tolerance && nb.dot(to_a).abs() <= tolerance
    }
}

pub struct Triangles {
    pub corners: Vec<[u32; 3]>,
    pub data: Vec<FaceData>,
}

impl Triangles {
    pub fn of(mesh: &Mesh) -> Triangles {
        let mut corners = Vec::with_capacity(mesh.triangle_count());
        let mut data = Vec::with_capacity(mesh.triangle_count());
        for (face, face_data) in mesh.faces.iter().zip(&mesh.face_data) {
            for tri in face.triangulate() {
                corners.push(tri);
                data.push(*face_data);
            }
        }
        Triangles { corners, data }
    }

    pub fn corner_at(&self, mesh: &Mesh, triangle: u32, position: u32) -> Option<u32> {
        self.corners[triangle as usize]
            .into_iter()
            .find(|&c| mesh.corners[c as usize].position == position)
    }
}

pub fn same_corner(a: &Corner, b: &Corner) -> bool {
    a.uvq == b.uvq && a.back_uvq == b.back_uvq && a.normal == b.normal
}
