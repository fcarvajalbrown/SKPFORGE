# skpforge

Converts SketchUp models into game-ready assets for Unreal Engine 5: repair, retopologise to quads, generate correct UV channels, bake from the original geometry, export.

The problem being solved is not "SketchUp has bad topology". It is that SketchUp UVs are per-face projective projections with no consistent texel density, which is unusable in a real-time engine. Everything else exists to serve that.

---

## Pipeline order — non-negotiable

```
IMPORT -> REPAIR -> ROUTE -> RETOPO -> UV -> BAKE -> EXPORT
```

Reordering breaks correctness, not just quality:

- UV before RETOPO is wasted work; retopo destroys vertex correspondence.
- BAKE before UV has nowhere to write.
- ROUTE before REPAIR reads density metrics off unwelded triangle soup and always picks wrong.

`skp-retopo` **must** emit a LOW-triangle to HIGH-triangle correspondence map as a first-class output. `skp-uv` and `skp-bake` both depend on it. It is not a debug artifact.

---

## Current state

Nothing is scaffolded yet. There is no `Cargo.toml`, no workspace, no crates, and no git repo. The table below is the plan, not the tree. The only source file that exists is `units.rs` at the repo root; it belongs at `skp-uv/src/units.rs` and moves there when the workspace is created.

---

## Workspace

| Crate | Responsibility |
|---|---|
| `skp-io` | SketchUp C SDK FFI, hierarchy flattening, UVQ extraction. Feature-gated. |
| `skp-repair` | Weld, orient windings, drop degenerates, coplanar merge, cull interior faces |
| `skp-retopo` | Route A/B, tri-to-quad pairing or quadriflow sidecar, correspondence map |
| `skp-uv` | UV0 reprojection, UV1 lightmap atlas, UV2 unique unwrap, validation |
| `skp-bake` | BVH, normal / AO / albedo transfer HIGH to LOW |
| `skp-export` | FBX and glTF writers, Unreal metadata sidecar |
| `skpforge-cli` | Headless batch entry point |
| `skpforge-ui` | egui + wgpu, HIGH/LOW split view |

---

## Build

```bash
cargo build                          # no SDK needed, skp-io stubbed
cargo build -p skp-io --features sdk # requires SKETCHUP_SDK_DIR
cargo test --workspace
cargo test -p skp-uv                 # one crate
cargo test -p skp-uv area_factor_is_not_the_linear_factor   # one test, substring match on the name
cargo test -p skp-uv -- --nocapture  # keep stdout from a failing geometry case
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

The SketchUp SDK is a licence acceptance, not a purchase. Download it, unpack to `vendor/sketchup-sdk/`, set `SKETCHUP_SDK_DIR`. It is gitignored and must never be committed. Windows and macOS only — Linux CI builds every crate except `skp-io`.

`vendor/xatlas` is a submodule compiled via `cc` in `skp-uv/build.rs`. Only `xatlas.cpp` and `xatlas.h` are needed; it has no external dependencies.

---

## Domain invariants

These are the things that are silently wrong rather than loudly broken. Treat every one as a hard rule.

### Units

- SketchUp is internally in **inches**. Unreal Units are **centimetres**. Factor is exactly `2.54`.
- Every scale factor lives in `skp-uv/src/units.rs`. Do not introduce a literal `2.54` anywhere else.
- Area scales by `2.54²  = 6.4516`, not `2.54`. `SqUu` is a distinct type specifically to make that mistake impossible.
- Texel density is **linear** (texels per cm). Texel count over an area is **density squared**. Use `TexelDensity::texels_for_area`, never `density * area`.
- `f64` everywhere internally. Narrow to `f32` only at export, via `Uu::as_f32`. SketchUp site models produce coordinates large enough that `f32` loses real precision.

### Coordinates

- SketchUp is right-handed Z-up. Unreal is left-handed Z-up. Negating Y converts, **and flips triangle winding**. Fix the winding in the same function that negates Y or normals invert.
- Unreal puts the V origin top-left; SketchUp puts it bottom-left. FBX and glTF exporters already flip V. Applying `flip_v` on top of that gives upside-down textures. Only raw writers call it.

### SketchUp UVs

- Coordinates come back as **UVQ**, projective. Always `u/q`, `v/q`.
- `q` can be exactly `0.0`. Guard every divide; do not assume it is finite.
- If `q` **varies across a face**, the texture is projectively distorted by the Texture Tweaker and no linear UV in Unreal can reproduce it. Detect via q-variance threshold and route those faces to a bake. This branch is mandatory, not an optimisation.
- Materials inherit from parent groups and components. Faces without a directly applied material behave differently through the UV helper. Resolution order is: face front material, then walk ancestors, then default.
- Faces carry separate front and back materials. Unreal is single-sided. Prefer front, flag back-only faces, offer auto-flip.

### UV channels

| Channel | Purpose | Overlap | Source |
|---|---|---|---|
| UV0 | Tiling material | Allowed and desirable | Analytic reprojection of SketchUp's plane projection |
| UV1 | Lightmap | **Never** | xatlas, packed, texel-snapped |
| UV2 | Baked detail | No | xatlas, optional |

- UV0 is **reprojected analytically, not baked**. SketchUp's projection is often at the correct world scale for tiling brick or siding. Extract the matrix, evaluate it on the LOW mesh. Baking that to texels throws away infinite resolution for nothing.
- Solve `MinLightmapResolution` **before** packing, then pack for exactly that texel grid with `blockAlign`. Packing at one resolution and rendering at another causes bleed at every chart border. Export the number in the sidecar so the artist does not have to set it by hand.
- Overlapping UV1 is a **hard pipeline failure**, not a warning. Lightmass renders it silently and it just looks wrong.

---

## Conventions

- **No comments.** The global zero-comment rule applies here too: no inline, block, or doc comments, no `TODO`/`FIXME`. Domain rationale lives in this file, not in the source. Existing comments in `units.rs` are debt awaiting deletion, never precedent.
- **Fix bugs at the root cause.** Never adjust a test parameter or add a workaround to make a test pass. If the geometry is wrong, fix the geometry.
- **Scaffold first, then one file at a time.** Define the full module structure before writing any file. Never dump multiple files in one go.
- **Diffs, not rewrites.** For fixes, give the changed snippet only. Never reproduce a whole file unless explicitly asked.
- **Decision questions as tappable options**, 2 to 4 mutually exclusive choices, recommended one marked `(rec)` with a short reason. Never prose bullet lists.
- Errors are `thiserror` per crate, `anyhow` only in the two binaries.
- Anything long-running takes a cancellation token and a progress callback. Remeshers report no progress, so the UI shows elapsed time and a working cancel, never a fake percentage.

---

## Licensing posture

- Own code: MIT. Keep it that way.
- `xatlas` is MIT. `quadriflow` is permissive. Both are safe to bundle.
- `quadwild-bimdf` is **GPL3**. It is an opt-in backend the user installs themselves, invoked as an external process. Never bundle it, never link it, never make it the default.
- The SketchUp SDK is closed and EULA-gated. Check redistribution terms before shipping its DLLs.
- Ship signed. An unsigned Windows binary that spawns child processes gets flagged hard by enterprise AV.

---

## Do not

- Do not run a field-aligned remesher on geometry that is already near target polycount. It rounds off 90 degree CAD corners and makes the silhouette worse than the input.
- Do not vendor, commit, or attempt to download the SketchUp SDK.
- Do not add a second literal unit conversion outside `units.rs`.
- Do not treat overlapping UV1 as recoverable.
- Do not assume an exported OBJ or FBX is equivalent to reading the `.skp`. Reading the source file is the entire point; it sidesteps unwelded vertices and material-split geometry.
