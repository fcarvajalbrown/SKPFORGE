# skpforge

Converts SketchUp models into game-ready assets for Unreal Engine 5: repair, retopologise to quads, generate correct UV channels, bake from the original geometry, export.

The problem being solved is not "SketchUp has bad topology". It is that SketchUp UVs are per-face projective projections with no consistent texel density, which is unusable in a real-time engine. Everything else exists to serve that.

---

## Pipeline order — non-negotiable

```
IMPORT -> REPAIR -> [DISPLACE] -> ROUTE -> RETOPO -> UV -> BAKE -> EXPORT
```

Reordering breaks correctness, not just quality:

- UV before RETOPO is wasted work; retopo destroys vertex correspondence.
- BAKE before UV has nowhere to write.
- ROUTE before REPAIR reads density metrics off unwelded triangle soup and always picks wrong.

DISPLACE is optional and off by default; a run without it is identical to a pipeline that never had the stage. Its position is forced all the same. After REPAIR, because welding is what makes a vertex shared, and displacing before it moves each face's own copy in a different direction, cracking every seam. Before ROUTE, so RETOPO sees the bent geometry and BAKE captures it through the correspondence map; displace after the bake and the normal map describes a surface that has moved.

`skp-retopo` **must** emit a LOW-triangle to HIGH-triangle correspondence map as a first-class output. `skp-uv` and `skp-bake` both depend on it. It is not a debug artifact.

---

## Current state

Phase 0 is done. The workspace is green: nine crates under `crates/`, zero external dependencies, `cargo test --workspace` passing with 40 tests in `skp-core` and none anywhere else.

`skp-core` holds four modules — `units.rs`, `mesh.rs`, `correspondence.rs` and `progress.rs` — and every other crate depends on it and on nothing else in the workspace. Nothing has been implemented in any of the other eight. `skp-displace` does not exist yet; it arrives in Phase 2b.

Phase 1 is IMPORT and is blocked on the SketchUp SDK being installed locally. See `ROADMAP.md`.

---

## Workspace

| Crate | Responsibility |
|---|---|
| `skp-core` | Mesh, attribute buffers, correspondence map, `units.rs`. Depends on nothing; everything depends on it |
| `skp-io` | SketchUp C SDK FFI, hierarchy flattening, UVQ extraction. Feature-gated. |
| `skp-repair` | Weld, orient windings, drop degenerates, coplanar merge, cull interior faces |
| `skp-displace` | Optional. Subdivide to displacement resolution, offset along a seeded noise field |
| `skp-retopo` | Route A/B, tri-to-quad pairing or quadriflow sidecar, correspondence map |
| `skp-uv` | UV0 reprojection, UV1 lightmap atlas, UV2 unique unwrap, validation |
| `skp-bake` | BVH, normal / AO / albedo transfer HIGH to LOW |
| `skp-export` | FBX and glTF writers, Unreal metadata sidecar |
| `skpforge-cli` | Headless batch entry point |
| `skpforge-ui` | HIGH/LOW split view. Toolkit undecided; egui and wgpu are ruled out by the dependency rule |

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
- Every scale factor lives in `skp-core/src/units.rs`. Do not introduce a literal `2.54` anywhere else.
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

## Dependencies — hard rule

**Nothing is taken from crates.io. Ever.** `[dependencies]` stays empty in every crate. This is not a preference to be weighed against convenience; it is the same rule for a two-line helper as for a rendering stack.

The reason is not that other people's code is bad. It is that one `cargo add` pulls a transitive tree nobody in this project has read, and a geometry pipeline whose correctness rests on unread code cannot be debugged when a model comes out subtly wrong.

- **Rust dependencies: none.** Errors are hand-written enums. Anything else a crate would have provided is written here or is not used.
- **C and C++ libraries: vendored, and read.** Self-contained sources only, into `vendor/<name>/`, committed, compiled through `cc` from the consuming crate's `build.rs`. `xatlas` is the model: two files, no external dependencies. If a library cannot be read before it is committed, it does not go in.
- **Size is never an objection**, and neither is rewriting something that already exists.

### The SketchUp SDK is the one exception

It lives at `vendor/sketchup-sdk/` and is the only thing under `vendor/` that is **not** committed. It is closed, EULA-gated, gitignored, and must never be vendored, committed, or downloaded — including by an agent acting on its own initiative. Felipe installs it and sets `SKETCHUP_SDK_DIR`.

The cost is that a fresh clone cannot build `skp-io`. That is already absorbed: `skp-io` is feature-gated behind `sdk` and stubbed by default, `cargo build` works without it, and Linux CI builds every crate except that one.

### What this rules out

`egui` and `wgpu` are not available to `skpforge-ui`, and no toolkit has replaced them yet. That decision is open and belongs to Phase 6, not to an assumption made earlier.

---

## Git

Commit as you go. One logical change per commit, and the workspace is green at every one. A session's work is never squashed into a single commit at the end.

- **Conventional Commits.** `<type>(<scope>): <subject>`, imperative, lowercase subject, no trailing period. Types: `feat`, `fix`, `refactor`, `test`, `docs`, `build`, `chore`.
- **Scope is the crate**, without the `skp-` prefix: `core`, `io`, `repair`, `displace`, `retopo`, `uv`, `bake`, `export`, `cli`, `ui`. Use `workspace` for root-level files, and omit the scope entirely when the change is repo-wide.
- **Green before every commit, no exceptions.** All three must pass:

```bash
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

  A commit that does not build is never acceptable, not even mid-phase. A later bisect lands on it.

- **One logical change per commit.** Moving a file and changing its contents are two commits. A new module and the wiring that exposes it are one.
- **The body says why.** The diff already says what.
- **Push after each commit** once a remote exists. There is no remote yet, so commits stay local.
- Never `--no-verify`. Never amend a pushed commit. No pull requests unless asked for in that turn.

Commits before `b37ebfd` predate this rule and use prose subjects. They are not rewritten.

---

## Conventions

- **No comments.** The global zero-comment rule applies here too: no inline, block, or doc comments, no `TODO`/`FIXME`. Domain rationale lives in this file, not in the source. Existing comments in `units.rs` are debt awaiting deletion, never precedent.
- **Fix bugs at the root cause.** Never adjust a test parameter or add a workaround to make a test pass. If the geometry is wrong, fix the geometry.
- **Scaffold first, then one file at a time.** Define the full module structure before writing any file. Never dump multiple files in one go.
- **Diffs, not rewrites.** For fixes, give the changed snippet only. Never reproduce a whole file unless explicitly asked.
- **Decision questions as tappable options**, 2 to 4 mutually exclusive choices, recommended one marked `(rec)` with a short reason. Never prose bullet lists.
- Errors are hand-written enums per crate, implementing `Display` and `std::error::Error` directly. No `thiserror`, no `anyhow`, no derive crates. See `MeshError` in `skp-core` for the shape.
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
