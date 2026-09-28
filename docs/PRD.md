# skpforge — Product Requirements

Status: Draft
Owner: Felipe Carvajal Brown

---

## 1. Problem

SketchUp is the fastest way to model architecture and props, and its output is close to unusable in a real-time engine. The common diagnosis is that the topology is bad. That is a symptom.

The actual defect is the UVs. SketchUp assigns texture coordinates as a per-face projective projection: each face carries its own plane projection, expressed as UVQ, and neighbouring faces have no relationship to one another. There is no consistent texel density anywhere in the model. A brick material that reads correctly on one wall reads at half scale on the wall it meets, and no amount of retopology fixes that, because retopology does not touch texture coordinates.

Unreal Engine 5 additionally requires a second, non-overlapping UV channel for lightmaps. SketchUp produces nothing of the kind. Overlapping lightmap UVs do not fail loudly; Lightmass renders them and the result simply looks wrong.

Everything else in skpforge — repair, routing, retopology, baking, export — exists to make correct UVs possible and to keep them correct through export.

## 2. Goals

1. Produce a mesh whose UV0 tiles at the world scale the SketchUp author intended, without baking that projection to texels.
2. Produce a UV1 lightmap channel with zero overlap, packed to the exact texel grid the model will be rendered at.
3. Preserve the visual detail of the original geometry through a bake when the retopologised mesh cannot carry it.
4. Read the `.skp` file directly, never an intermediate export.
5. Fail loudly on the conditions that are silently wrong in every other toolchain, chiefly overlapping UV1 and projectively distorted faces.

## 3. Non-goals

These are out of scope for v1.0 and are named so they are not drifted into.

| Not doing | Why |
|---|---|
| FBX export | A binary FBX writer is a large reverse-engineering effort against a closed format, and subtle errors are invisible until Unreal misreads them. Deferred to v2. |
| Batch processing a folder | v1.0 handles one model per invocation. Batch is a wrapper, not a capability. |
| Mesh editing in the UI | The UI inspects; it does not modify geometry. |
| Material authoring | skpforge carries SketchUp's materials through. It does not create, merge or author them. |
| Baking anything but albedo | Material baking is albedo only. Roughness, metallic and emissive are separate decisions, not yet made. |
| Linux support for the full pipeline | The SketchUp SDK is Windows and macOS only. Linux builds every crate except `skp-io`. |
| Bundling quadwild-bimdf | GPL3. Opt-in, user-installed, invoked as an external process, never linked and never the default. |

## 4. Users

Built for my own work: models I make in SketchUp that have to end up in Unreal Engine 5 as lit, textured assets.

No other user is assumed. Where a default is arguable, it is set to what my models need, not to what is generally safe.

## 5. v1.0 scope

The whole pipeline, end to end, on a single model, driven from the CLI.

```
skpforge-cli model.skp -o model.glb
```

| Stage | Delivers |
|---|---|
| IMPORT | Flattened hierarchy, UVQ per face, resolved materials, front and back material distinction |
| REPAIR | Welded vertices, consistent windings, degenerates dropped, coplanar faces merged, interior faces culled |
| DISPLACE | **Off by default.** Seeded subdivision and vertex offset, so walls and timber are not perfectly straight |
| ROUTE | Route A or Route B chosen from metrics on the welded mesh, both metrics recorded |
| RETOPO | Quad-dominant LOW mesh, plus the LOW-to-HIGH correspondence map |
| UV | UV0 reprojected analytically, UV1 packed and validated non-overlapping, UV2 where a bake needs it |
| BAKE | Normal and ambient occlusion transferred HIGH to LOW. Albedo atlas on UV2 when asked for |
| EXPORT | glTF 2.0 binary, plus the Unreal metadata sidecar |

`skpforge-ui` in v1.0 is a HIGH/LOW split viewer. It does not launch or edit.

## 6. Architecture

### 6.1 Pipeline order

```
IMPORT -> REPAIR -> [DISPLACE] -> ROUTE -> RETOPO -> UV -> BAKE -> EXPORT
```

The order is a correctness constraint, not a preference. UV before RETOPO is discarded work, because retopology destroys vertex correspondence. BAKE before UV has nowhere to write. ROUTE before REPAIR reads its density metrics off unwelded triangle soup and picks wrong every time.

DISPLACE is the one optional stage. Skipping it produces the same result as a pipeline that never had it.

### 6.2 DISPLACE

Off by default, enabled per run. It exists because walls and timber are never straight, and geometry that is exactly straight reads as computer output.

It sits after REPAIR and before ROUTE, and both boundaries are forced:

- **After REPAIR**, because welding is what makes a vertex shared between the faces that meet at it. Displace before welding and each face moves its own copy of the vertex in a different direction, so the model cracks open along every seam.
- **Before ROUTE**, so RETOPO sees the bent geometry and BAKE captures the imperfection through the correspondence map. Displacing the LOW mesh after the bake instead means the normal map is describing a surface that is no longer there, and it cancels the bow out in shading while leaving it in the silhouette.

A SketchUp wall is a quad. Four vertices cannot bow, only tilt, so the stage subdivides to a displacement resolution before offsetting anything. Offsets come from a seeded noise field, so the same seed and the same input give the same output.

The cost is paid in UV0. Analytic reprojection evaluates a plane projection on the mesh, and a plane projection assumes the surface lies on the plane. Bowing the wall means it does not, so the tiling stretches across the bulge. At a few millimetres the error is second-order and invisible; at a visible bow it is the non-uniform texel density this tool exists to remove. The amplitude is therefore bounded and the resulting UV0 distortion is measured, not assumed.

### 6.3 Dependency direction

`skp-core` holds the types every stage shares — the mesh representation, the attribute buffers, the correspondence map, and every unit type and scale factor. It depends on nothing and builds on every platform. Every other crate depends on it and on nothing else in the workspace, except the two binaries, which depend on the stages they drive.

This keeps dependency direction aligned with pipeline order, and it keeps the six stage crates independent of `skp-io`, which is SDK-gated and cannot build on Linux CI at all.

### 6.4 The correspondence map

`skp-retopo` emits a LOW-triangle to HIGH-triangle correspondence map as a first-class output, carried in `skp-core`. `skp-uv` and `skp-bake` both consume it. It is not a debug artifact and it is not optional.

### 6.5 Routing

ROUTE computes two metrics on the welded mesh and picks from both:

```
ratio = input_tris / target_tris
sharp = fraction of edges with dihedral angle > 30 degrees,
        open and coplanar edges left out

ratio <= 1.5                    -> Route A, already near target
sharp >= 0.35                   -> Route A, CAD-like, corners must survive
ratio >  3.0 and sharp < 0.35   -> Route B, organic and heavy
otherwise                       -> Route A with decimation
```

Route A is tri-to-quad pairing. Route B is the field-aligned remesher, QuadriFlow ported to Rust and run in-process (ADRs 0004 and 0006).

`target_tris` comes from `--target-tris`, and defaults to the input count, so a run without a budget is never reduced. A non-manifold edge counts by the widest angle between any two faces at it. An open edge has no dihedral. A coplanar edge, two faces in one plane within the weld tolerance, is a triangulation artefact rather than shape, and counting it made models with large flat faces read less sharp than their corners are.

The sharpness gate exists because polycount alone gets the important case backwards. A 200,000-triangle building is far over budget and would route to the remesher, which is exactly the input that must not go there: every corner in it is 90 degrees and a field-aligned remesh rounds all of them, making the silhouette worse than the input it replaced.

`--route a|b|auto` overrides, with `auto` the default. The chosen route and both metrics are written to the sidecar.

### 6.6 Material baking

Off by default, enabled per run. Scope for now is **albedo and nothing else** — no roughness, no metallic, no emissive. Those are separate decisions and are not being made yet.

A SketchUp model routinely carries dozens of materials, and each one is a draw call in Unreal. BAKE can flatten them: sample the resolved albedo off the HIGH mesh through the correspondence map and write it into a single atlas on UV2, so the model exports with one texture and one material.

This runs alongside UV0, never instead of it. UV0 stays the analytic tiling projection, unbaked, because baking a brick projection to texels trades infinite resolution for a fixed one and gets nothing back. The two coexist in the export and the choice of which to drive the Unreal material with is made per asset, not per pipeline.

### 6.7 Export

glTF 2.0 binary. The three UV channels map to `TEXCOORD_0`, `TEXCOORD_1` and `TEXCOORD_2` in that order, which is a specified part of the format rather than a convention. Unreal 5 reads it through Interchange.

The sidecar carries what glTF has no field for, `MinLightmapResolution` first among them, so the artist does not set it by hand and does not get it wrong.

## 7. Invariants

Restated from CLAUDE.md because they are requirements, not style. CLAUDE.md remains the authority; this section exists so the PRD is readable standalone.

- SketchUp is in inches, an Unreal Unit is a centimetre, the factor is exactly 2.54, and it appears in exactly one file.
- Area scales by 2.54 squared. `SqUu` exists as a distinct type to make the linear factor inapplicable.
- Texel density is linear, texels per centimetre. Texel count over an area is density squared.
- `f64` internally, narrowed to `f32` only at export. Site models produce coordinates where `f32` loses real precision.
- Converting handedness negates Y, which flips triangle winding. Both happen in the same function or normals invert.
- UVQ is projective. Every divide by `q` is guarded, because `q` can be exactly zero.
- A face whose `q` varies is projectively distorted and no linear UV can reproduce it. Those faces route to a bake. This branch is mandatory.
- Overlapping UV1 is a pipeline failure, not a warning.

## 8. Success criteria for v1.0

One model, chosen up front, goes from `.skp` to `.glb` and looks right. Judged in two steps, in this order:

1. **In `skpforge-ui` first.** The result is inspected in the split view before it leaves the tool: UV0 tiling at the right scale, UV1 with no overlap, the bake landing where it should, the LOW silhouette against the HIGH. Anything wrong is visible here, in a viewer that knows what it is looking at, rather than in an engine that does not.
2. **Then in Unreal 5.** Imports through Interchange, three UV channels in order, `MinLightmapResolution` from the sidecar, and it looks right when lit.

This makes the UI part of the acceptance path rather than an afterthought, which is why it moves ahead of export in the roadmap.

Anything the pipeline aborts on counts as working, not failing. Refusing to export overlapping UV1 is the feature.

## 9. Risks

| Risk | Consequence | Mitigation |
|---|---|---|
| SketchUp SDK licence terms restrict redistributing its DLLs | Cannot ship a working binary | Check terms before shipping; the check is a phase gate, not an afterthought |
| Analytic UV0 reprojection does not survive retopology cleanly | The core promise of the tool fails | Correspondence map is a first-class output specifically to make this evaluable early |
| Field-aligned remeshing quality on architectural input | Route B produces worse output than its input | The sharpness gate keeps that input on Route A; Route B is the narrow case |
| Unsigned Windows binary spawning child processes | Enterprise AV flags it hard | Ship signed |
