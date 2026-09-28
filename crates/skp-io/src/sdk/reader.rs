use super::ffi::*;
use crate::error::IoError;
use crate::scene::{
    LoadStatus, ModelUnits, Node, NodeKind, Scene, SourceFace, SourceVertex, Transform,
};
use skp_core::mesh::{Material, MaterialId, Uvq};
use skp_core::progress::CancelToken;
use skp_core::units::Inches;
use std::collections::HashMap;
use std::ffi::{c_char, CString};
use std::path::Path;

fn check(call: &'static str, code: SUResult) -> Result<(), IoError> {
    if code == SU_ERROR_NONE {
        return Ok(());
    }
    Err(IoError::Sdk { call, code })
}

fn optional<T: Copy>(call: &'static str, code: SUResult, value: T) -> Result<Option<T>, IoError> {
    match code {
        SU_ERROR_NONE => Ok(Some(value)),
        SU_ERROR_NO_DATA => Ok(None),
        _ => Err(IoError::Sdk { call, code }),
    }
}

fn list<T: Copy + Default>(
    call: &'static str,
    count: impl FnOnce(*mut usize) -> SUResult,
    get: impl FnOnce(usize, *mut T, *mut usize) -> SUResult,
) -> Result<Vec<T>, IoError> {
    let mut n = 0usize;
    check(call, count(&mut n))?;
    fill(call, n, get)
}

fn fill<T: Copy + Default>(
    call: &'static str,
    n: usize,
    get: impl FnOnce(usize, *mut T, *mut usize) -> SUResult,
) -> Result<Vec<T>, IoError> {
    let mut items = vec![T::default(); n];
    if n == 0 {
        return Ok(items);
    }
    let mut got = 0usize;
    check(call, get(n, items.as_mut_ptr(), &mut got))?;
    items.truncate(got);
    Ok(items)
}

struct Session;

impl Session {
    fn start() -> Self {
        unsafe { SUInitialize() };
        Session
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe { SUTerminate() };
    }
}

struct Model(SUModelRef);

impl Drop for Model {
    fn drop(&mut self) {
        unsafe { SUModelRelease(&mut self.0) };
    }
}

struct MeshHelper(SUMeshHelperRef);

impl Drop for MeshHelper {
    fn drop(&mut self) {
        unsafe { SUMeshHelperRelease(&mut self.0) };
    }
}

struct SuString(SUStringRef);

impl SuString {
    fn new() -> Result<Self, IoError> {
        let mut s = SUStringRef::default();
        check("SUStringCreate", unsafe { SUStringCreate(&mut s) })?;
        Ok(SuString(s))
    }

    fn to_string_lossy(&self) -> Result<String, IoError> {
        let mut len = 0usize;
        check("SUStringGetUTF8Length", unsafe {
            SUStringGetUTF8Length(self.0, &mut len)
        })?;
        let mut buf = vec![0u8; len + 1];
        let mut copied = 0usize;
        check("SUStringGetUTF8", unsafe {
            SUStringGetUTF8(
                self.0,
                buf.len(),
                buf.as_mut_ptr() as *mut c_char,
                &mut copied,
            )
        })?;
        buf.truncate(copied.min(len));
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}

impl Drop for SuString {
    fn drop(&mut self) {
        unsafe { SUStringRelease(&mut self.0) };
    }
}

pub fn read_scene(path: &Path, cancel: &CancelToken) -> Result<Scene, IoError> {
    let c_path = path
        .to_str()
        .and_then(|s| CString::new(s).ok())
        .ok_or(IoError::PathNotUtf8)?;
    let _session = Session::start();
    let mut model = Model(SUModelRef::default());
    let mut status = 0;
    check("SUModelCreateFromFileWithStatus", unsafe {
        SUModelCreateFromFileWithStatus(&mut model.0, c_path.as_ptr(), &mut status)
    })?;
    read_model(model.0, status, cancel)
}

pub(crate) fn read_model(
    model: SUModelRef,
    status: i32,
    cancel: &CancelToken,
) -> Result<Scene, IoError> {
    let mut reader = Reader {
        materials: Vec::new(),
        ids: HashMap::new(),
        hidden_skipped: 0,
        cancel,
    };
    let model_materials = list(
        "SUModelGetMaterials",
        |n| unsafe { SUModelGetNumMaterials(model, n) },
        |len, out, got| unsafe { SUModelGetMaterials(model, len, out, got) },
    )?;
    for material in model_materials {
        reader.material_id(material)?;
    }

    let mut units = 0;
    check("SUModelGetUnits", unsafe {
        SUModelGetUnits(model, &mut units)
    })?;
    let mut entities = SUEntitiesRef::default();
    check("SUModelGetEntities", unsafe {
        SUModelGetEntities(model, &mut entities)
    })?;
    let root = reader.node(NodeKind::Root, Transform::IDENTITY, None, entities)?;

    Ok(Scene {
        materials: reader.materials,
        root,
        load_status: if status == SU_MODEL_LOAD_STATUS_SUCCESS_MORE_RECENT {
            LoadStatus::NewerThanSdk
        } else {
            LoadStatus::Current
        },
        units: match units {
            SU_MODEL_UNITS_FEET => ModelUnits::Feet,
            SU_MODEL_UNITS_MILLIMETERS => ModelUnits::Millimetres,
            SU_MODEL_UNITS_CENTIMETERS => ModelUnits::Centimetres,
            SU_MODEL_UNITS_METERS => ModelUnits::Metres,
            SU_MODEL_UNITS_INCHES => ModelUnits::Inches,
            _ => ModelUnits::Inches,
        },
        hidden_skipped: reader.hidden_skipped,
    })
}

struct Reader<'a> {
    materials: Vec<Material>,
    ids: HashMap<usize, MaterialId>,
    hidden_skipped: usize,
    cancel: &'a CancelToken,
}

impl Reader<'_> {
    fn material_id(&mut self, material: SUMaterialRef) -> Result<MaterialId, IoError> {
        let key = material.ptr as usize;
        if let Some(&id) = self.ids.get(&key) {
            return Ok(id);
        }
        let mut name = SuString::new()?;
        check("SUMaterialGetNameLegacyBehavior", unsafe {
            SUMaterialGetNameLegacyBehavior(material, &mut name.0)
        })?;
        let id = MaterialId(self.materials.len() as u32);
        self.materials.push(Material {
            name: name.to_string_lossy()?,
        });
        self.ids.insert(key, id);
        Ok(id)
    }

    fn resolve(
        &mut self,
        call: &'static str,
        code: SUResult,
        material: SUMaterialRef,
    ) -> Result<Option<MaterialId>, IoError> {
        match optional(call, code, material)? {
            Some(m) if !m.ptr.is_null() => Ok(Some(self.material_id(m)?)),
            _ => Ok(None),
        }
    }

    fn is_hidden(&mut self, element: SUDrawingElementRef) -> Result<bool, IoError> {
        let mut hidden = false;
        check("SUDrawingElementGetHidden", unsafe {
            SUDrawingElementGetHidden(element, &mut hidden)
        })?;
        let mut layer = SULayerRef::default();
        let mut visible = true;
        let has_layer = unsafe { SUDrawingElementGetLayer(element, &mut layer) } == SU_ERROR_NONE
            && !layer.ptr.is_null();
        if has_layer {
            check("SULayerGetVisibility", unsafe {
                SULayerGetVisibility(layer, &mut visible)
            })?;
        }
        let skip = hidden || !visible;
        if skip {
            self.hidden_skipped += 1;
        }
        Ok(skip)
    }

    fn element_material(
        &mut self,
        element: SUDrawingElementRef,
    ) -> Result<Option<MaterialId>, IoError> {
        let mut material = SUMaterialRef::default();
        let code = unsafe { SUDrawingElementGetMaterial(element, &mut material) };
        self.resolve("SUDrawingElementGetMaterial", code, material)
    }

    fn node(
        &mut self,
        kind: NodeKind,
        transform: Transform,
        material: Option<MaterialId>,
        entities: SUEntitiesRef,
    ) -> Result<Node, IoError> {
        let mut node = Node {
            kind,
            transform,
            material,
            ..Node::default()
        };

        let faces = list(
            "SUEntitiesGetFaces",
            |n| unsafe { SUEntitiesGetNumFaces(entities, n) },
            |len, out, got| unsafe { SUEntitiesGetFaces(entities, len, out, got) },
        )?;
        for face in faces {
            self.cancel.check()?;
            if self.is_hidden(unsafe { SUFaceToDrawingElement(face) })? {
                continue;
            }
            node.faces.push(self.face(face)?);
        }

        let groups = list(
            "SUEntitiesGetGroups",
            |n| unsafe { SUEntitiesGetNumGroups(entities, n) },
            |len, out, got| unsafe { SUEntitiesGetGroups(entities, len, out, got) },
        )?;
        for group in groups {
            let element = unsafe { SUGroupToDrawingElement(group) };
            if self.is_hidden(element)? {
                continue;
            }
            let mut t = SUTransformation::default();
            check("SUGroupGetTransform", unsafe {
                SUGroupGetTransform(group, &mut t)
            })?;
            let mut inner = SUEntitiesRef::default();
            check("SUGroupGetEntities", unsafe {
                SUGroupGetEntities(group, &mut inner)
            })?;
            let material = self.element_material(element)?;
            node.children
                .push(self.node(NodeKind::Group, Transform(t.values), material, inner)?);
        }

        let instances = list(
            "SUEntitiesGetInstances",
            |n| unsafe { SUEntitiesGetNumInstances(entities, n) },
            |len, out, got| unsafe { SUEntitiesGetInstances(entities, len, out, got) },
        )?;
        for instance in instances {
            let element = unsafe { SUComponentInstanceToDrawingElement(instance) };
            if self.is_hidden(element)? {
                continue;
            }
            let mut t = SUTransformation::default();
            check("SUComponentInstanceGetTransform", unsafe {
                SUComponentInstanceGetTransform(instance, &mut t)
            })?;
            let mut definition = SUComponentDefinitionRef::default();
            check("SUComponentInstanceGetDefinition", unsafe {
                SUComponentInstanceGetDefinition(instance, &mut definition)
            })?;
            let mut inner = SUEntitiesRef::default();
            check("SUComponentDefinitionGetEntities", unsafe {
                SUComponentDefinitionGetEntities(definition, &mut inner)
            })?;
            let material = self.element_material(element)?;
            node.children.push(self.node(
                NodeKind::Instance,
                Transform(t.values),
                material,
                inner,
            )?);
        }

        Ok(node)
    }

    fn face(&mut self, face: SUFaceRef) -> Result<SourceFace, IoError> {
        let mut front = SUMaterialRef::default();
        let code = unsafe { SUFaceGetFrontMaterial(face, &mut front) };
        let front = self.resolve("SUFaceGetFrontMaterial", code, front)?;
        let mut back = SUMaterialRef::default();
        let code = unsafe { SUFaceGetBackMaterial(face, &mut back) };
        let back = self.resolve("SUFaceGetBackMaterial", code, back)?;

        let mut helper = MeshHelper(SUMeshHelperRef::default());
        check("SUMeshHelperCreate", unsafe {
            SUMeshHelperCreate(&mut helper.0, face)
        })?;
        let h = helper.0;
        let positions = list(
            "SUMeshHelperGetVertices",
            |n| unsafe { SUMeshHelperGetNumVertices(h, n) },
            |len, out, got| unsafe { SUMeshHelperGetVertices(h, len, out, got) },
        )?;
        let n = positions.len();
        let normals = fill("SUMeshHelperGetNormals", n, |len, out, got| unsafe {
            SUMeshHelperGetNormals(h, len, out, got)
        })?;
        let front_stq = fill("SUMeshHelperGetFrontSTQCoords", n, |len, out, got| unsafe {
            SUMeshHelperGetFrontSTQCoords(h, len, out, got)
        })?;
        let back_stq = fill("SUMeshHelperGetBackSTQCoords", n, |len, out, got| unsafe {
            SUMeshHelperGetBackSTQCoords(h, len, out, got)
        })?;
        let mut triangle_count = 0usize;
        check("SUMeshHelperGetNumTriangles", unsafe {
            SUMeshHelperGetNumTriangles(h, &mut triangle_count)
        })?;
        let indices = fill(
            "SUMeshHelperGetVertexIndices",
            triangle_count * 3,
            |len, out, got| unsafe { SUMeshHelperGetVertexIndices(h, len, out, got) },
        )?;

        let stq = |coords: &[SUPoint3D], i: usize| {
            coords
                .get(i)
                .map(|p| Uvq {
                    u: p.x,
                    v: p.y,
                    q: p.z,
                })
                .unwrap_or_default()
        };
        let vertices = positions
            .iter()
            .enumerate()
            .map(|(i, p)| SourceVertex {
                position: [Inches(p.x), Inches(p.y), Inches(p.z)],
                normal: normals.get(i).map(|v| [v.x, v.y, v.z]).unwrap_or_default(),
                front: stq(&front_stq, i),
                back: stq(&back_stq, i),
            })
            .collect();
        if let Some(&index) = indices.iter().find(|&&i| i >= n) {
            return Err(IoError::TessellationOutOfRange { index, vertices: n });
        }
        let triangles = indices
            .chunks_exact(3)
            .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
            .collect();

        Ok(SourceFace {
            vertices,
            triangles,
            front,
            back,
        })
    }
}
