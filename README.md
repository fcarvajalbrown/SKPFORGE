<p align="center"><img src="assets/logo.svg" alt="skpforge logo, an isometric anvil with a block resting on it" width="128"></p>
<h1 align="center">skpforge</h1>
<p align="center"><strong>SketchUp to Unreal Engine 5 asset pipeline, written in Rust. Repair, quad retopology, correct tiling and lightmap UVs, baking, glTF export.</strong></p>
<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-1E4E8C.svg" alt="License: MIT"></a>
  <img src="https://img.shields.io/badge/rust-1.90%2B-F26B1D.svg?logo=rust" alt="Rust 1.90+">
  <img src="https://img.shields.io/badge/Unreal%20Engine-5-1E4E8C.svg?logo=unrealengine" alt="Unreal Engine 5">
  <img src="https://img.shields.io/badge/crates.io%20dependencies-0-F26B1D.svg" alt="Zero crates.io dependencies">
  <img src="https://img.shields.io/badge/status-early%20development-C4500F.svg" alt="Status: early development">
</p>

> Early development. Nothing runs end to end yet. See [Status](#status).

---

## The problem this solves

Everyone says SketchUp has bad topology. That is a symptom, and fixing it does not get you a usable asset.

The real defect is the UVs. SketchUp assigns texture coordinates as a per-face projective projection, so every face carries its own plane projection and neighbouring faces have no relationship to each other. There is no consistent texel density anywhere in the model. A brick material that reads correctly on one wall reads at half scale on the wall it meets. Retopology will not fix that, because retopology does not touch texture coordinates.

Unreal also needs a second, non-overlapping UV channel for lightmaps. SketchUp produces nothing of the kind. Overlapping lightmap UVs do not fail loudly either. Lightmass renders them and the result just looks wrong.

So skpforge treats UVs as the point and everything else as the support: repair, routing, retopology and baking exist to make correct UVs possible and to keep them correct through export.

## What it does

```
IMPORT -> REPAIR -> [DISPLACE] -> ROUTE -> RETOPO -> UV -> BAKE -> EXPORT
```

| Stage | What it delivers |
|---|---|
| IMPORT | Reads the `.skp` directly through the SketchUp C SDK. Flattened hierarchy, UVQ per face, resolved materials |
| REPAIR | Welds vertices, makes windings consistent, drops degenerates, merges coplanar faces, culls interior geometry |
| DISPLACE | Optional, off by default. Seeded subdivision and vertex offset, because walls and timber are never straight |
| ROUTE | Picks tri-to-quad pairing or a field-aligned remesh from polycount ratio gated by sharp-edge fraction |
| RETOPO | Quad-dominant low-poly mesh, plus a low-to-high correspondence map |
| UV | UV0 reprojected analytically, UV1 lightmap packed and validated, UV2 where a bake needs it |
| BAKE | Normal and ambient occlusion transferred high to low. Optional albedo atlas |
| EXPORT | glTF 2.0 binary, plus an Unreal metadata sidecar |

### The three UV channels

| Channel | Purpose | Overlap | How it is made |
|---|---|---|---|
| UV0 | Tiling material | Allowed, and wanted | Analytic reprojection of SketchUp's own plane projection |
| UV1 | Lightmap | Never | xatlas, packed and texel-snapped |
| UV2 | Baked detail | No | xatlas, optional |

Two details worth calling out, because most toolchains get them wrong:

**UV0 is reprojected, not baked.** SketchUp's projection is frequently already at the right world scale for tiling brick or siding. Extract the matrix and evaluate it on the new mesh. Baking that to texels trades infinite resolution for a fixed one and gets nothing back.

**`MinLightmapResolution` is solved before packing, not after.** Pack at one resolution and render at another and you get bleed at every chart border. The number goes in the sidecar so it does not have to be set by hand.

## Status

Three of eight phases are done and a fourth has started. IMPORT reads `.skp` files through the SketchUp SDK and `inspect` prints what it found. REPAIR welds, fixes windings, drops degenerates, culls interior faces and merges coplanar ones, and `repair` reports what each step changed. ROUTE works: `route` measures the model and says which retopology route it would take and why. RETOPO itself is next.

| Phase | State |
|---|---|
| 0 — Workspace and core types | Done |
| 1 — IMPORT | Done |
| 2 — REPAIR | Done |
| 2b — DISPLACE (optional) | Not started |
| 3 — ROUTE and RETOPO | In progress, ROUTE done |
| 4 — UV | Not started |
| 5 — BAKE | Not started |
| 6 — UI | Not started |
| 7 — EXPORT | Not started |

Full detail in [ROADMAP.md](ROADMAP.md). Scope and architecture in [PRD.md](docs/PRD.md).

## Build

```bash
cargo build                          # no SDK needed, skp-io is stubbed
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Building the SketchUp reader needs the SDK:

```bash
cargo build -p skp-io --features sdk # requires SKETCHUP_SDK_DIR
cargo run -p skpforge-cli --features sdk -- inspect model.skp
cargo run -p skpforge-cli --features sdk -- repair model.skp
cargo run -p skpforge-cli --features sdk -- route model.skp --target-tris 20000
```

`inspect` prints what IMPORT read: faces, triangles, groups, component instances, mirrored placements, material counts, back-only faces, faces whose texture `q` varies, and the bounds in centimetres, followed by a per-material table.

`repair` prints the same import report, then what REPAIR changed: positions welded, degenerates dropped, triangles turned over, interior faces culled, coplanar triangles merged away, and a count of open, inconsistently wound and non-manifold edges before and after. `--weld-tolerance <cm>` overrides the default of 0.001 inch, SketchUp's own merge distance.

`route` runs `repair` and then prints the two routing numbers. `ratio` is the repaired triangle count over `--target-tris`. Leave the flag out and the target is the input count, so nothing gets reduced. `sharp` is the share of edges bending more than 30 degrees, with open edges and edges between two faces in the same plane left out, since a quad's diagonal says nothing about the shape. The last line names the route and the rule that picked it. `--route a|b|auto` forces a route. On the three test models, sharp came out at 0.955, 0.984 and 0.578, and all three stay on tri-to-quad pairing.

### Dependencies

skpforge takes nothing from crates.io. `[dependencies]` is empty in every crate, errors are hand-written enums, and C or C++ libraries are vendored as self-contained sources under `vendor/`, read before they are committed, and compiled through `cc`. The SketchUp SDK is the sole exception: it is closed and EULA-gated, so it is gitignored and installed by you rather than vendored.

The SketchUp SDK is a licence acceptance rather than a purchase. Download it, unpack to `vendor/sketchup-sdk/`, and set `SKETCHUP_SDK_DIR`. It is gitignored and is never committed. Windows and macOS only; on Linux every crate builds except `skp-io`.

## Workspace

| Crate | Responsibility |
|---|---|
| `skp-core` | Mesh, attribute buffers, correspondence map, units, triangle geometry and the edge map. Depends on nothing |
| `skp-io` | SketchUp C SDK FFI, hierarchy flattening, UVQ extraction |
| `skp-repair` | Weld, orient windings, drop degenerates, coplanar merge, interior culling |
| `skp-displace` | Optional. Subdivide and offset along a seeded noise field |
| `skp-retopo` | Route A/B, quad pairing or remesh sidecar, correspondence map |
| `skp-uv` | UV0 reprojection, UV1 lightmap atlas, UV2 unwrap, validation |
| `skp-bake` | BVH, normal / AO / albedo transfer high to low |
| `skp-export` | glTF writer, Unreal metadata sidecar |
| `skpforge-cli` | Headless entry point |
| `skpforge-ui` | High/low split view. Toolkit not yet decided |

## Licence

MIT. See [LICENSE](LICENSE).

`xatlas` is MIT and is bundled. `quadwild-bimdf` is GPL3, so it is never bundled or linked; it is an opt-in backend you install yourself and skpforge invokes as an external process. The SketchUp SDK is closed and EULA-gated, and is not redistributed here.
