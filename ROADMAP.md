# skpforge — Roadmap

Phase-based. No dates. A phase is `Not Started`, `In Progress`, `Blocked` or `Done`.

Phases run in pipeline order because the pipeline order is a correctness constraint.

The UI is not last. PRD section 8 makes it the first half of the acceptance test — the result is inspected in the split view before it ever reaches Unreal — so it has to exist before EXPORT can be signed off.

---

## Phase 0 — Workspace and core types

Status: **Done**

`skp-core` exists and holds every type the stages share, and nothing depends on anything outside the workspace.

- [x] Workspace, eight crates, zero external dependencies
- [x] `skp-io` gated behind the `sdk` feature
- [x] `units.rs` under version control with its six tests passing
- [x] `skp-core` crate created, workspace becomes nine members
- [x] `units.rs` moved `skp-uv/src` to `skp-core/src`, CLAUDE.md's unit-file reference updated in the same commit
- [x] `Mesh` representation: positions, indices, per-face and per-vertex attribute buffers
- [x] `Correspondence` type, LOW triangle to HIGH triangle
- [x] Every stage crate depends on `skp-core` and on nothing else in the workspace
- [x] Hand-written error enums per crate implementing `Display` and `std::error::Error`, no dependency crates
- [x] Cancellation token and progress callback traits, since every long-running stage takes them

Exit: `cargo test --workspace` green, `cargo clippy --workspace --all-targets -- -D warnings` clean, and the dependency graph matches PRD 6.3.

Related: [ADR 0001](docs/adr/0001-correspondence-map-representation.md) on the correspondence map representation. ADR on the `skp-core` split and ADR on the cancellation and progress interface both still to be written.

---

## Phase 1 — IMPORT

Status: **Done**

Depends on: Phase 0. Requires the SketchUp SDK locally.

- [x] SDK acquisition documented, `SKETCHUP_SDK_DIR`, `vendor/sketchup-sdk/` gitignored
- [x] FFI bindings behind the `sdk` feature, stub path builds without it
- [x] Hierarchy flattening: groups, components, instance transforms
- [x] UVQ extraction per face, every divide by `q` guarded against exactly zero
- [x] q-variance computed per face and carried on the mesh, since UV consumes it later
- [x] Material resolution: face front material, then walk ancestors, then default
- [x] Front and back materials kept distinct, back-only faces flagged
- [x] Inches to `Uu` at the boundary, nothing downstream sees inches
- [x] Exit test: `skpforge-cli inspect` on a real textured model checked against SketchUp's Model Info

Exit: a real `.skp` reads into a `skp-core::Mesh` with materials resolved, and the diagnostic report matches what SketchUp shows for the same file.

Signed off against three real models: `Casa Neoclasica.skp`, `Estación de Salamanca.skp` (saved by a newer SketchUp than the 2021 SDK) and `3d66.com_1154175.skp` (286,861 faces, 10,370 component instances, 2,267 mirrored, textured materials). Counts matched SketchUp once hidden entities and per-placement counting were allowed for.

Related: ADR on the FFI and stub strategy, ADR on material resolution order. Both still to be written.

---

## Phase 2 — REPAIR

Status: **In Progress**

Depends on: Phase 1.

- [x] Vertex welding with a position tolerance. The weld searches neighbouring grid cells, so points within tolerance on either side of a cell boundary still join, and a chain of close points does not weld end to end
- [x] Winding orientation made consistent across the mesh. Orientation propagates only across manifold edges; closed shells are turned to positive signed volume, open surfaces keep the winding of most of their area. Turning a triangle over swaps front and back material, UVQ and q variance, which is why `FaceData` now carries a q variance for each side
- [x] Degenerate triangles dropped: a repeated position, or a height below the weld tolerance. Exact duplicates (same positions, same winding) are dropped too; the same positions wound the other way are left for interior culling
- [ ] Coplanar face merging
- [x] Interior face culling. Candidates are planar patches whose outer edges are all shared by three or more faces or by a reversed twin; one is culled only when the generalised winding number of the rest of the mesh puts both its sides inside a solid, so a pane across a window opening survives. Touching solids whose contact faces only partly overlap are not handled, since that needs a boolean
- [ ] Report of what was changed, per operation

Exit: welded, consistently wound output on which the ROUTE metrics are meaningful.

Related: [ADR 0002](docs/adr/0002-weld-tolerance.md) on the weld tolerance.

---

## Phase 2b — DISPLACE

Status: Not Started

Depends on: Phase 2. **Optional stage, off by default.** Skipping this phase entirely does not block anything downstream, which is why it carries a letter rather than a number.

- [ ] `skp-displace` crate, depends only on `skp-core`
- [ ] Subdivision to a displacement resolution, since a four-vertex quad cannot bow
- [ ] Seeded noise field, `--seed` reproducible across runs
- [ ] Vertex offset with amplitude in `Uu`, bounded
- [ ] Shared vertices move once, verified by a test that the mesh stays watertight
- [ ] UV0 distortion introduced by a given amplitude **measured**, not assumed, and reported
- [ ] `--displace` off by default, and the pipeline without it is byte-identical to one that never had the stage

Exit: a wall visibly sags in the silhouette, the mesh has no cracks at seams, and the UV0 stretch across the bow is quantified.

Related: ADR on the noise field and where amplitude is specified (per material, per run, or per surface class).

---

## Phase 3 — ROUTE and RETOPO

Status: Not Started

Depends on: Phase 2. Sees displaced geometry if Phase 2b ran.

- [ ] `ratio` and `sharp` metrics computed on the welded mesh
- [ ] Router implementing the PRD 6.5 table, `--route a|b|auto`, both metrics recorded
- [ ] Route A: tri-to-quad pairing, with decimation
- [ ] Route B: field-aligned remesher as a sidecar process, elapsed time and working cancel, never a fake percentage
- [ ] **Correspondence map emitted by both routes**, as a first-class output
- [ ] Correspondence validated: every LOW triangle maps to at least one HIGH triangle

Exit: a CAD-like model over budget routes to A with its corners intact; a heavy organic model routes to B; both emit a valid correspondence map.

Related: ADR on how Route B obtains a quadriflow binary (vendored and built with `cc`, or user-installed sidecar). The correspondence map representation is settled in [ADR 0001](docs/adr/0001-correspondence-map-representation.md).

---

## Phase 4 — UV

Status: Not Started

Depends on: Phase 3, and specifically on the correspondence map.

- [ ] Extract the SketchUp plane projection matrix per face
- [ ] UV0 reprojected analytically onto the LOW mesh, not baked
- [ ] Faces with varying `q` routed to a bake instead of reprojected. On `3d66.com_1154175.skp`, an untweaked textured model, 59 faces showed a nonzero q-variance with a maximum of 3.7e-13, which is rounding noise, so the threshold has to sit well above that
- [ ] `xatlas` submodule vendored, built via `cc` in `skp-uv/build.rs`
- [ ] `MinLightmapResolution` solved **before** packing
- [ ] UV1 packed to that exact texel grid with `blockAlign`
- [ ] UV1 overlap validation, and it aborts the pipeline rather than warning
- [ ] UV2 unique unwrap where a bake needs it

Exit: UV0 texel density on a known-size face matches the SketchUp original, and UV1 overlap is provably zero.

Related: ADR on the `MinLightmapResolution` solver, ADR on the q-variance threshold.

---

## Phase 5 — BAKE

Status: Not Started

Depends on: Phase 4.

- [ ] BVH over the HIGH mesh
- [ ] Normal transfer HIGH to LOW
- [ ] Ambient occlusion
- [ ] Albedo transfer, for the projectively distorted faces routed here from Phase 4
- [ ] `--bake-materials`, off by default: every SketchUp material sampled into one albedo atlas on UV2
- [ ] Atlas resolution configurable, material count before and after reported
- [ ] UV0 left untouched by all of the above
- [ ] Cancellation and progress throughout

Albedo only. Roughness, metallic and emissive are explicitly not in scope.

Exit: distortion-routed faces render correctly from baked texture where no linear UV could reproduce them, and a model with dozens of materials exports as one when asked.

---

## Phase 6 — UI

Status: Not Started

Depends on: Phase 3, and can start there. Must be finished before Phase 7 can be signed off, because it is the first half of the acceptance test.

- [ ] Decide the rendering approach. `egui` and `wgpu` are ruled out by the dependency rule, so this needs its own ADR before any UI code
- [ ] Window and render shell on whatever that decision lands on
- [ ] HIGH/LOW split view
- [ ] UV0 inspected: tiling scale readable against a known-size face
- [ ] UV1 inspected: overlap made visible rather than merely reported
- [ ] UV2 and the bakes inspected, including the albedo atlas when one was made
- [ ] LOW silhouette against HIGH, which is where a bad Route B shows up
- [ ] Route metrics and chosen route displayed
- [ ] Viewer only: no editing, no launching a run

Exit: a finished run can be judged right or wrong here, without opening Unreal.

---

## Phase 7 — EXPORT

Status: Not Started

Depends on: Phase 5 to produce, Phase 6 to sign off.

- [ ] glTF 2.0 binary writer
- [ ] `TEXCOORD_0`, `TEXCOORD_1`, `TEXCOORD_2` in channel order
- [ ] Handedness conversion and winding flip in the same function
- [ ] V flip applied exactly once, not on top of the writer's own
- [ ] `f64` narrowed to `f32` here and nowhere earlier
- [ ] Sidecar with `MinLightmapResolution`, chosen route, both route metrics
- [ ] Unique material names. IMPORT reads names with `SUMaterialGetNameLegacyBehavior`, which strips a surrounding `[...]`, and `3d66.com_1154175.skp` has two distinct materials that both come out as `Color_007`
- [ ] End-to-end CLI: `skpforge-cli model.skp -o model.glb`

Exit, both halves, in order: the run is inspected in `skpforge-ui` and judged right, then the `.glb` imports into Unreal 5 with three UV channels in order and the sidecar's lightmap resolution applied. This is v1.0.

Related: ADR on the sidecar schema.

---

## After v1.0

Not scheduled, listed so they are not mistaken for scope.

- FBX 7.x binary writer
- Baking roughness, metallic and emissive, which v1 deliberately leaves out
- Batch processing a folder
- `quadwild-bimdf` as an opt-in Route B backend, user-installed, external process, never bundled
- Code signing for the Windows binary
