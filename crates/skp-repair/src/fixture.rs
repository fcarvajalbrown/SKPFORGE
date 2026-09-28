use skp_core::mesh::{Corner, Face, FaceData, Mesh, Normal, Point, Uvq};

pub fn soup(points: &[Point]) -> Mesh {
    Mesh {
        positions: points.to_vec(),
        corners: (0..points.len() as u32)
            .map(|position| Corner {
                position,
                ..Corner::default()
            })
            .collect(),
        faces: (0..points.len() as u32 / 3)
            .map(|t| Face::Tri([3 * t, 3 * t + 1, 3 * t + 2]))
            .collect(),
        face_data: vec![FaceData::default(); points.len() / 3],
        materials: Vec::new(),
    }
}

pub fn indexed(points: &[Point], triangles: &[[u32; 3]]) -> Mesh {
    let mut mesh = Mesh {
        positions: points.to_vec(),
        ..Mesh::default()
    };
    for tri in triangles {
        let [a, b, c] = tri.map(|i| points[i as usize]);
        let normal = unit_normal(a, b, c);
        let first = mesh.corners.len() as u32;
        for &position in tri {
            let p = points[position as usize];
            mesh.corners.push(Corner {
                position,
                uvq: Uvq {
                    u: p.x.0,
                    v: p.y.0,
                    q: 1.0,
                },
                back_uvq: Uvq {
                    u: -p.x.0,
                    v: p.y.0,
                    q: 1.0,
                },
                normal,
            });
        }
        mesh.faces.push(Face::Tri([first, first + 1, first + 2]));
        mesh.face_data.push(FaceData::default());
    }
    mesh
}

pub fn position_triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
    mesh.faces
        .iter()
        .map(|f| {
            let c = f.corners();
            [c[0], c[1], c[2]].map(|corner| mesh.corners[corner as usize].position)
        })
        .collect()
}

fn unit_normal(a: Point, b: Point, c: Point) -> Normal {
    let u = [b.x.0 - a.x.0, b.y.0 - a.y.0, b.z.0 - a.z.0];
    let v = [c.x.0 - a.x.0, c.y.0 - a.y.0, c.z.0 - a.z.0];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len == 0.0 {
        return Normal::default();
    }
    Normal {
        x: n[0] / len,
        y: n[1] / len,
        z: n[2] / len,
    }
}
