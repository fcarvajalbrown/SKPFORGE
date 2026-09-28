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

Status: **Done**

Depends on: Phase 1.

- [x] Vertex welding with a position tolerance. The weld searches neighbouring grid cells, so points within tolerance on either side of a cell boundary still join, and a chain of close points does not weld end to end
- [x] Winding orientation made consistent across the mesh. Orientation propagates only across manifold edges; closed shells are turned to positive signed volume, open surfaces keep the winding of most of their area. Turning a triangle over swaps front and back material, UVQ and q variance, which is why `FaceData` now carries a q variance for each side
- [x] Degenerate triangles dropped: a repeated position, or a height below the weld tolerance. Exact duplicates (same positions, same winding) are dropped too; the same positions wound the other way are left for interior culling. Nearly every degenerate on real models is a zero-area needle from a collinear vertex in a SketchUp face loop. Dropping one outright leaves a T-junction, so the triangle across its long edge is split at the needle's middle vertex, with UVQ interpolated linearly, which is exact. A split that would leave a half below the tolerance is refused, otherwise slivers split into slivers without end (it exhausted memory on `3d66.com_1154175.skp`)
- [x] Coplanar face merging, by removing vertices rather than rebuilding polygons. A vertex goes when every face around it shares one plane, one material pair, no q variance, and one affine UV and back-UV mapping, or when it sits on a straight seam or crease between two such regions. A collapse is refused if it would flip, thin or bend a triangle or break the link condition, so coverage, silhouette and UV0 are unchanged. Faces with any q variance are left alone until Phase 4 settles the threshold
- [x] Interior face culling. Candidates are planar patches whose outer edges are all shared by three or more faces or by a reversed twin. One is culled only when both its sides are inside a closed shell by the generalised winding number, and the field is built only from shells that are closed once the candidates are set aside. Against every face it put 271 visible triangles of the single-sheet `Casa Neoclasica.skp` "inside", which is why open geometry no longer counts. A pane across a window opening survives, and so does a sheet partition in room air. Touching solids whose contact faces only partly overlap are not handled, since that needs a boolean
- [x] Report of what was changed, per operation, plus an edge census (open, manifold, inconsistently wound, non-manifold) after the weld and at the end. `skpforge-cli repair <model.skp> [--weld-tolerance <cm>]` prints it, with elapsed time per stage on stderr

Exit: welded, consistently wound output on which the ROUTE metrics are meaningful.

Checked against the three Phase 1 models, release build:

| Model | Triangles | Open edges | Inconsistently wound | Non-manifold | Culled | Merged away | Time |
|---|---|---|---|---|---|---|---|
| `Casa Neoclasica.skp` | 5,061 -> 5,057 | 29 -> 29 | 8 -> 0 | 1,334 -> 1,334 | 0 | 4 | 0.02 s |
| `Estación de Salamanca.skp` | 5,202 -> 4,961 | 65 -> 40 | 4 -> 0 | 139 -> 135 | 4 | 239 | 0.03 s |
| `3d66.com_1154175.skp` | 702,658 -> 662,843 | 107,710 -> 97,312 | 10,336 -> 27 | 14,520 -> 14,060 | 226 | 39,421 | 8.5 s |

The "before" edge counts are taken after welding. REPAIR never adds an open or non-manifold edge on these models. `Casa Neoclasica.skp` is drawn in single-sheet walls, so its 1,334 non-manifold edges are T-junctions of real walls, and it has no closed shell to cull against. The 27 inconsistent edges left on `3d66.com_1154175.skp` are conflicts the orientation walk cannot resolve, most likely surfaces that welding joined into something locally non-orientable. They were not inspected one by one. Whether what was culled and merged looks right has only been checked through these counts, not visually; that belongs to the Phase 6 viewer.

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

Status: **In Progress**. ROUTE is done. RETOPO has Route A pairing; decimation and Route B are open.

Depends on: Phase 2. Sees displaced geometry if Phase 2b ran.

- [x] `ratio` and `sharp` metrics computed on the welded mesh
- [x] Router implementing the PRD 6.5 table, `--route a|b|auto`, both metrics recorded. `skpforge-cli route <model.skp> [--weld-tolerance <cm>] [--target-tris <n>] [--route a|b|auto]` prints them with the table row that decided; writing them to the sidecar waits for Phase 7

Settled before any code, since PRD 6.5 leaves both open:

- `target_tris` comes from `--target-tris <n>`. Omitted, it equals the input triangle count, so `ratio` is 1.0 and the model routes to A with pairing only. Nothing is reduced unless a budget is asked for, and no default budget is invented.
- `sharp` counts a manifold edge by its dihedral angle and a non-manifold edge by the largest angle between any two faces that meet at it, so the T-junctions of single-sheet walls count as the corners they are. Open edges have no dihedral and are left out of both numerator and denominator; their count is reported beside the metric.

Metrics on the three Phase 1 models after REPAIR, release build:

| Model | Triangles | sharp | Sharp / measured edges | Open edges left out |
|---|---|---|---|---|
| `Casa Neoclasica.skp` | 5,057 | 0.426 | 2,939 / 6,891 | 29 |
| `Estación de Salamanca.skp` | 4,961 | 0.575 | 4,197 / 7,293 | 40 |
| `3d66.com_1154175.skp` | 662,843 | 0.329 | 308,955 / 938,032 | 97,312 |

With no budget all three route to A as already near target. Given one, the two small models stay on A by the sharpness gate (`Casa Neoclasica.skp` at `--target-tris 1000`), but `3d66.com_1154175.skp` at `--target-tris 100000` goes to Route B, 0.021 under the gate. The denominator counts every edge inside a flat region too, such as the diagonal that splits a quad into two triangles, and those are never sharp, so a model with large flat faces reads less sharp than its corners are. Whether B is the wrong answer for this model depends on what it contains, which has not been checked.

Checked against published sources (web search, 2026-09-28). A dihedral threshold near 30 degrees is a common choice for feature edges, so that half of `sharp` has precedent. Using the fraction of edges that are sharp to tell CAD from organic geometry, and the 0.35 cutoff, has none that was found; they are this PRD's own. The sources describe CAD models as large smooth or flat regions separated by a sparse set of sharp creases, which means the per-edge fraction depends on how finely the flat regions are triangulated, not only on the shape. QuadriFlow does not detect sharp edges by default; it has a `-sharp` flag, and a Blender issue reports that option leaving holes and non-manifold triangles. That supports keeping CAD input off Route B, and it is the case this gate exists for.

Decided from that: `sharp` now also leaves out coplanar edges, those whose two faces face the same way with each far vertex within the weld tolerance of the other face's plane. They come from triangulation, not shape, and an organic mesh has almost none. A fin folded flat back on itself is not coplanar and stays sharp. 30 degrees and 0.35 are unchanged. The table above is the metric before this change; after it, at `--target-tris 1000`:

| Model | sharp | Sharp / measured edges | Coplanar left out | Route |
|---|---|---|---|---|
| `Casa Neoclasica.skp` | 0.955 | 2,939 / 3,076 | 3,815 | A, CAD-like |
| `Estación de Salamanca.skp` | 0.984 | 4,197 / 4,265 | 3,028 | A, CAD-like |
| `3d66.com_1154175.skp` | 0.578 | 308,955 / 534,257 | 403,775 | A, CAD-like |

The sharp edge counts are identical before and after, as they must be: only the denominator changed. What 3d66 contains is still unchecked, but it now clears the gate by 0.228 rather than missing it by 0.021. No organic model has been measured yet, so how far below 0.35 organic input lands under this definition is unknown until one is.
- [ ] Route A: tri-to-quad pairing, with decimation. Pairing is settled as coplanar only and exact: two triangles pair across a manifold, consistently wound, coplanar edge (ROUTE's test, weld tolerance), with equal face data, identical corners at the shared edge, and a convex quad, most rectangular first. The quad's diagonal is the original shared edge, so each LOW triangle is exactly one HIGH triangle and shape and UV0 are untouched. Curved regions stay triangles until decimation, whose method is still open

Pairing is built (`skp-retopo/src/pair.rs`) and emits its correspondence map through skp-core's `Correspondence`, validated before it is returned. `skpforge-cli retopo <model.skp>` takes the same flags as `route`, runs pairing when the decision is A, and stops with a message when it is B. On the three Phase 1 models after REPAIR, release build, no budget:

| Model | HIGH triangles | Pairable edges | Quads | Triangles left | Pair time |
|---|---|---|---|---|---|
| `Casa Neoclasica.skp` | 5,057 | 2,452 | 2,039 | 979 | 0.00 s |
| `Estación de Salamanca.skp` | 4,961 | 2,473 | 2,341 | 279 | 0.01 s |
| `3d66.com_1154175.skp` | 662,843 | 338,311 | 287,428 | 87,987 | 1.17 s |

Twice the quads plus the triangles left equals the HIGH count on each, as it must with no triangle dropped or added. Pairable edges are coplanar edges that also pass the face data, corner and convexity checks; fewer quads than pairable edges is the greedy choice, since each triangle joins at most one quad. Whether the quads look right has not been checked visually.
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
