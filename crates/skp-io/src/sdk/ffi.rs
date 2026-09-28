#![allow(non_snake_case)]

use std::ffi::{c_char, c_int, c_void};
use std::ptr;

pub type SUResult = c_int;
pub const SU_ERROR_NONE: SUResult = 0;
pub const SU_ERROR_NO_DATA: SUResult = 9;

pub const SU_MODEL_LOAD_STATUS_SUCCESS_MORE_RECENT: c_int = 1;

pub const SU_MODEL_UNITS_INCHES: c_int = 0;
pub const SU_MODEL_UNITS_FEET: c_int = 1;
pub const SU_MODEL_UNITS_MILLIMETERS: c_int = 2;
pub const SU_MODEL_UNITS_CENTIMETERS: c_int = 3;
pub const SU_MODEL_UNITS_METERS: c_int = 4;

macro_rules! su_ref {
    ($($name:ident),* $(,)?) => {
        $(
            #[repr(C)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub struct $name {
                pub ptr: *mut c_void,
            }

            impl Default for $name {
                fn default() -> Self {
                    $name { ptr: ptr::null_mut() }
                }
            }
        )*
    };
}

su_ref!(
    SUModelRef,
    SUEntitiesRef,
    SUFaceRef,
    SUGroupRef,
    SUComponentInstanceRef,
    SUComponentDefinitionRef,
    SUDrawingElementRef,
    SUMaterialRef,
    SUMeshHelperRef,
    SULayerRef,
    SUStringRef,
);

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SUPoint3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SUVector3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SUTransformation {
    pub values: [f64; 16],
}

extern "C" {
    pub fn SUInitialize();
    pub fn SUTerminate();

    pub fn SUStringCreate(out: *mut SUStringRef) -> SUResult;
    pub fn SUStringRelease(s: *mut SUStringRef) -> SUResult;
    pub fn SUStringGetUTF8Length(s: SUStringRef, out_length: *mut usize) -> SUResult;
    pub fn SUStringGetUTF8(
        s: SUStringRef,
        len: usize,
        out: *mut c_char,
        out_copied: *mut usize,
    ) -> SUResult;

    pub fn SUModelCreateFromFileWithStatus(
        model: *mut SUModelRef,
        file_path: *const c_char,
        status: *mut c_int,
    ) -> SUResult;
    pub fn SUModelRelease(model: *mut SUModelRef) -> SUResult;
    pub fn SUModelGetEntities(model: SUModelRef, entities: *mut SUEntitiesRef) -> SUResult;
    pub fn SUModelGetNumMaterials(model: SUModelRef, count: *mut usize) -> SUResult;
    pub fn SUModelGetMaterials(
        model: SUModelRef,
        len: usize,
        materials: *mut SUMaterialRef,
        count: *mut usize,
    ) -> SUResult;
    pub fn SUModelGetUnits(model: SUModelRef, units: *mut c_int) -> SUResult;

    pub fn SUEntitiesGetNumFaces(entities: SUEntitiesRef, count: *mut usize) -> SUResult;
    pub fn SUEntitiesGetFaces(
        entities: SUEntitiesRef,
        len: usize,
        faces: *mut SUFaceRef,
        count: *mut usize,
    ) -> SUResult;
    pub fn SUEntitiesGetNumGroups(entities: SUEntitiesRef, count: *mut usize) -> SUResult;
    pub fn SUEntitiesGetGroups(
        entities: SUEntitiesRef,
        len: usize,
        groups: *mut SUGroupRef,
        count: *mut usize,
    ) -> SUResult;
    pub fn SUEntitiesGetNumInstances(entities: SUEntitiesRef, count: *mut usize) -> SUResult;
    pub fn SUEntitiesGetInstances(
        entities: SUEntitiesRef,
        len: usize,
        instances: *mut SUComponentInstanceRef,
        count: *mut usize,
    ) -> SUResult;

    pub fn SUFaceToDrawingElement(face: SUFaceRef) -> SUDrawingElementRef;
    pub fn SUFaceGetFrontMaterial(face: SUFaceRef, material: *mut SUMaterialRef) -> SUResult;
    pub fn SUFaceGetBackMaterial(face: SUFaceRef, material: *mut SUMaterialRef) -> SUResult;

    pub fn SUMeshHelperCreate(mesh: *mut SUMeshHelperRef, face: SUFaceRef) -> SUResult;
    pub fn SUMeshHelperRelease(mesh: *mut SUMeshHelperRef) -> SUResult;
    pub fn SUMeshHelperGetNumTriangles(mesh: SUMeshHelperRef, count: *mut usize) -> SUResult;
    pub fn SUMeshHelperGetNumVertices(mesh: SUMeshHelperRef, count: *mut usize) -> SUResult;
    pub fn SUMeshHelperGetVertexIndices(
        mesh: SUMeshHelperRef,
        len: usize,
        indices: *mut usize,
        count: *mut usize,
    ) -> SUResult;
    pub fn SUMeshHelperGetVertices(
        mesh: SUMeshHelperRef,
        len: usize,
        vertices: *mut SUPoint3D,
        count: *mut usize,
    ) -> SUResult;
    pub fn SUMeshHelperGetFrontSTQCoords(
        mesh: SUMeshHelperRef,
        len: usize,
        stq: *mut SUPoint3D,
        count: *mut usize,
    ) -> SUResult;
    pub fn SUMeshHelperGetBackSTQCoords(
        mesh: SUMeshHelperRef,
        len: usize,
        stq: *mut SUPoint3D,
        count: *mut usize,
    ) -> SUResult;
    pub fn SUMeshHelperGetNormals(
        mesh: SUMeshHelperRef,
        len: usize,
        normals: *mut SUVector3D,
        count: *mut usize,
    ) -> SUResult;

    pub fn SUGroupToDrawingElement(group: SUGroupRef) -> SUDrawingElementRef;
    pub fn SUGroupGetTransform(group: SUGroupRef, transform: *mut SUTransformation) -> SUResult;
    pub fn SUGroupGetEntities(group: SUGroupRef, entities: *mut SUEntitiesRef) -> SUResult;

    pub fn SUComponentInstanceToDrawingElement(
        instance: SUComponentInstanceRef,
    ) -> SUDrawingElementRef;
    pub fn SUComponentInstanceGetTransform(
        instance: SUComponentInstanceRef,
        transform: *mut SUTransformation,
    ) -> SUResult;
    pub fn SUComponentInstanceGetDefinition(
        instance: SUComponentInstanceRef,
        definition: *mut SUComponentDefinitionRef,
    ) -> SUResult;
    pub fn SUComponentDefinitionGetEntities(
        definition: SUComponentDefinitionRef,
        entities: *mut SUEntitiesRef,
    ) -> SUResult;

    pub fn SUDrawingElementGetMaterial(
        element: SUDrawingElementRef,
        material: *mut SUMaterialRef,
    ) -> SUResult;
    pub fn SUDrawingElementGetLayer(
        element: SUDrawingElementRef,
        layer: *mut SULayerRef,
    ) -> SUResult;
    pub fn SUDrawingElementGetHidden(element: SUDrawingElementRef, hidden: *mut bool) -> SUResult;
    pub fn SULayerGetVisibility(layer: SULayerRef, visible: *mut bool) -> SUResult;

    pub fn SUMaterialGetNameLegacyBehavior(
        material: SUMaterialRef,
        name: *mut SUStringRef,
    ) -> SUResult;
}
