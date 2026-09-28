use super::ffi::*;
use super::reader::read_model;
use crate::flatten::flatten;
use skp_core::mesh::{MaterialId, Mesh};
use skp_core::progress::{CancelToken, NoProgress};
use std::ffi::{c_char, CString};

extern "C" {
    fn SUModelCreate(model: *mut SUModelRef) -> SUResult;
    fn SUModelAddMaterials(
        model: SUModelRef,
        len: usize,
        materials: *const SUMaterialRef,
    ) -> SUResult;
    fn SUMaterialCreate(material: *mut SUMaterialRef) -> SUResult;
    fn SUMaterialSetName(material: SUMaterialRef, name: *const c_char) -> SUResult;
    fn SUFaceCreateSimple(face: *mut SUFaceRef, vertices: *const SUPoint3D, len: usize)
        -> SUResult;
    fn SUFaceSetFrontMaterial(face: SUFaceRef, material: SUMaterialRef) -> SUResult;
    fn SUEntitiesAddFaces(entities: SUEntitiesRef, len: usize, faces: *const SUFaceRef)
        -> SUResult;
    fn SUGroupCreate(group: *mut SUGroupRef) -> SUResult;
    fn SUEntitiesAddGroup(entities: SUEntitiesRef, group: SUGroupRef) -> SUResult;
    fn SUGroupSetTransform(group: SUGroupRef, transform: *const SUTransformation) -> SUResult;
    fn SUDrawingElementSetMaterial(
        element: SUDrawingElementRef,
        material: SUMaterialRef,
    ) -> SUResult;
}

fn ok(code: SUResult) {
    assert_eq!(code, SU_ERROR_NONE);
}

fn p(x: f64, y: f64, z: f64) -> SUPoint3D {
    SUPoint3D { x, y, z }
}

fn material(name: &str) -> SUMaterialRef {
    let mut m = SUMaterialRef::default();
    let name = CString::new(name).unwrap();
    unsafe {
        ok(SUMaterialCreate(&mut m));
        ok(SUMaterialSetName(m, name.as_ptr()));
    }
    m
}

fn face(points: &[SUPoint3D]) -> SUFaceRef {
    let mut f = SUFaceRef::default();
    unsafe { ok(SUFaceCreateSimple(&mut f, points.as_ptr(), points.len())) };
    f
}

fn id_of(mesh: &Mesh, name: &str) -> MaterialId {
    let index = mesh.materials.iter().position(|m| m.name == name).unwrap();
    MaterialId(index as u32)
}

fn winding_agrees_with_normal(mesh: &Mesh, face: usize) -> bool {
    let c = mesh.faces[face].corners();
    let q = |i: usize| mesh.corner_position(c[i]).unwrap();
    let (a, b, d) = (q(0), q(1), q(2));
    let u = [(b.x - a.x).0, (b.y - a.y).0, (b.z - a.z).0];
    let v = [(d.x - a.x).0, (d.y - a.y).0, (d.z - a.z).0];
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let n = mesh.corners[c[0] as usize].normal;
    cross[0] * n.x + cross[1] * n.y + cross[2] * n.z > 0.0
}

#[test]
fn authored_model_round_trips_through_the_sdk() {
    unsafe { SUInitialize() };
    let mut model = SUModelRef::default();
    unsafe { ok(SUModelCreate(&mut model)) };
    let red = material("Red");
    let blue = material("Blue");
    unsafe { ok(SUModelAddMaterials(model, 2, [red, blue].as_ptr())) };

    let mut root = SUEntitiesRef::default();
    unsafe { ok(SUModelGetEntities(model, &mut root)) };
    let ground = face(&[
        p(0.0, 0.0, 0.0),
        p(10.0, 0.0, 0.0),
        p(10.0, 10.0, 0.0),
        p(0.0, 10.0, 0.0),
    ]);
    unsafe { ok(SUEntitiesAddFaces(root, 1, &ground)) };

    let mut group = SUGroupRef::default();
    unsafe {
        ok(SUGroupCreate(&mut group));
        ok(SUEntitiesAddGroup(root, group));
    }
    let mut inner = SUEntitiesRef::default();
    unsafe { ok(SUGroupGetEntities(group, &mut inner)) };
    let painted = face(&[
        p(0.0, 0.0, 5.0),
        p(4.0, 0.0, 5.0),
        p(4.0, 4.0, 5.0),
        p(0.0, 4.0, 5.0),
    ]);
    let bare = face(&[
        p(0.0, 0.0, 0.0),
        p(0.0, 4.0, 0.0),
        p(0.0, 4.0, 4.0),
        p(0.0, 0.0, 4.0),
    ]);
    unsafe {
        ok(SUFaceSetFrontMaterial(painted, blue));
        ok(SUEntitiesAddFaces(inner, 2, [painted, bare].as_ptr()));
    }
    let mirror_and_shift = SUTransformation {
        values: [
            -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 100.0, 0.0, 0.0, 1.0,
        ],
    };
    unsafe {
        ok(SUGroupSetTransform(group, &mirror_and_shift));
        ok(SUDrawingElementSetMaterial(
            SUGroupToDrawingElement(group),
            red,
        ));
    }

    let scene = read_model(model, 0, &CancelToken::new()).unwrap();
    let (mesh, report) = flatten(&scene, &CancelToken::new(), &NoProgress).unwrap();
    unsafe {
        ok(SUModelRelease(&mut model));
        SUTerminate();
    }

    assert_eq!(report.faces, 3);
    assert_eq!(report.triangles, 6);
    assert_eq!(report.groups, 1);
    assert_eq!(report.mirrored_nodes, 1);
    assert_eq!(report.materials_in_model, 2);
    assert_eq!(report.materials_used, 2);
    assert_eq!(report.default_material_faces, 1);

    let (red, blue) = (id_of(&mesh, "Red"), id_of(&mesh, "Blue"));
    let painted: Vec<_> = mesh
        .face_data
        .iter()
        .filter(|d| d.front == Some(blue))
        .collect();
    assert_eq!(painted.len(), 2);
    assert!(painted.iter().all(|d| d.back == Some(red)));
    let inherited = mesh
        .face_data
        .iter()
        .filter(|d| d.front == Some(red) && d.back == Some(red))
        .count();
    assert_eq!(inherited, 2);

    let (lo, hi) = report.bounds.unwrap();
    assert_eq!(lo.x.0, 0.0);
    let painted_xs: Vec<f64> = mesh
        .faces
        .iter()
        .zip(&mesh.face_data)
        .filter(|(_, d)| d.front == Some(blue))
        .flat_map(|(f, _)| f.corners().to_vec())
        .map(|c| mesh.corner_position(c).unwrap().x.0)
        .collect();
    assert!(painted_xs
        .iter()
        .all(|x| (x - 96.0 * 2.54).abs() < 1e-9 || (x - 100.0 * 2.54).abs() < 1e-9));
    assert!(painted_xs.iter().any(|x| (x - 96.0 * 2.54).abs() < 1e-9));
    assert!((hi.x.0 - 100.0 * 2.54).abs() < 1e-9, "{hi:?}");
    assert!((hi.z.0 - 5.0 * 2.54).abs() < 1e-9, "{hi:?}");

    for index in 0..mesh.faces.len() {
        assert!(winding_agrees_with_normal(&mesh, index), "face {index}");
    }
}
