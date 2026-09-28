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

Status: **In Progress**. ROUTE, Route A and Route B are done; on Route A the triangle budget is advisory. Route B is a Rust port of QuadriFlow per ADR 0004, checked against upstream on two tori. Its max flow is Boykov-Kolmogorov per ADR 0006. Open: an organic model for the exit criterion.

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
- [x] Route A: tri-to-quad pairing, with decimation. Pairing is settled as coplanar only and exact: two triangles pair across a manifold, consistently wound, coplanar edge (ROUTE's test, weld tolerance), with equal face data, identical corners at the shared edge, and a convex quad, most rectangular first. The quad's diagonal is the original shared edge, so each LOW triangle is exactly one HIGH triangle and shape and UV0 are untouched. Curved regions stay triangles until decimation. Decimation is settled as quadric-error edge collapse, run only when the model is over `--target-tris`, and before pairing. Edges over 30 degrees, open edges, and material or UV seams are never collapsed, so CAD corners and UV0 boundaries survive. Each LOW triangle's correspondence is the set of HIGH triangles that collapsed into it

Pairing is built (`skp-retopo/src/pair.rs`) and emits its correspondence map through skp-core's `Correspondence`, validated before it is returned. `skpforge-cli retopo <model.skp>` takes the same flags as `route`, runs pairing when the decision is A, and stops with a message when it is B. On the three Phase 1 models after REPAIR, release build, no budget:

| Model | HIGH triangles | Pairable edges | Quads | Triangles left | Pair time |
|---|---|---|---|---|---|
| `Casa Neoclasica.skp` | 5,057 | 2,452 | 2,039 | 979 | 0.00 s |
| `Estación de Salamanca.skp` | 4,961 | 2,473 | 2,341 | 279 | 0.01 s |
| `3d66.com_1154175.skp` | 662,843 | 338,311 | 287,428 | 87,987 | 1.17 s |

Twice the quads plus the triangles left equals the HIGH count on each, as it must with no triangle dropped or added. Pairable edges are coplanar edges that also pass the face data, corner and convexity checks; fewer quads than pairable edges is the greedy choice, since each triangle joins at most one quad. Whether the quads look right has not been checked visually.

Decimation is built (`skp-retopo/src/decimate.rs`) as half-edge quadric collapse: a vertex slides onto a neighbour, so no position or UVQ value is invented. Every endpoint of a locked edge is pinned. A collapse is refused unless the vertex has one closed fan, the link condition holds, and no surviving triangle flips, turns past 30 degrees or drops below the weld tolerance. Each removed triangle hands its HIGH triangles to its neighbour across the edge. `route_a.rs` runs decimation when over budget, then pairing, and composes the two maps into one from LOW to the input. `skpforge-cli retopo` runs it. On a sphere fixture it reaches half the triangle count with no fold and a closed result.

On the three Phase 1 models it does almost nothing. Release build, `--target-tris 1000` for the two small models and `100000` for 3d66:

| Model | Pinned vertices | Locked edges by first rule: open or non-manifold / inconsistent / material / UV seam / normal seam / sharp | Collapses |
|---|---|---|---|
| `Casa Neoclasica.skp` | 1,869 of 1,869 | 1,363 / 0 / 16 / 1,971 / 14 / 0 | 0 |
| `Estación de Salamanca.skp` | 2,769 of 2,769 | 175 / 0 / 0 / 3,973 / 211 / 0 | 0 |
| `3d66.com_1154175.skp` | 382,037 of 382,074 | 111,372 / 27 / 17,958 / 520,170 / 596 / 2 | 1 |

Each locked edge is counted under the first rule that locks it, in that order, so a sharp edge that is also a UV seam counts as a UV seam; SketchUp projects UVs per face, so almost every edge between faces in different planes is one. To see whether the UV rule is what blocks it, the UV and normal seam locks were switched off for one throwaway run, not committed. The small models stayed fully pinned, now by 1,594 and 4,062 sharp edges; 3d66 freed 21,417 vertices and made 18,275 collapses, 662,843 to 626,293 triangles against a target of 100,000. So on this input the blocker is geometry, not UVs: after REPAIR's coplanar merge, every vertex left is a corner of some planar face and sits on a crease or an open edge. Locked-feature collapse cannot reach a CAD budget on SketchUp models.

Decided from that: on Route A the budget is advisory. Decimation stays as built and reduces only where no locked edge moves; `retopo` then prints the gap, for example `budget missed by 4057 triangles` on `Casa Neoclasica.skp` at `--target-tris 1000`. Every vertex left is modelled shape rather than triangulation, so cutting further means deleting detail, which is a level-of-detail decision for the artist and not retopo's. Sliding collinear crease vertices and dropping small detail by size were considered and not taken.
- [x] Route B: QuadriFlow ported to Rust (ADR 0004), elapsed time and working cancel, never a fake percentage

Upstream `src/` read at `810b7a0`, 9,232 lines. What its default run, `quadriflow -i in.obj -o out.obj -f <faces>`, executes:

- Subdivide to a target edge length, directed-edge structure, uniform adjacency, a multi-resolution hierarchy built by graph colouring and downsampling (seeded pcg32, a parallel stable sort).
- Orientation field, then orientation singularities. The scale solve is skipped unless `-adaptive`, but `main.cpp` sets the adaptive flag to 1 after it, so every later position solve runs with scale.
- Position field, position singularities, then the index map: edge info, integer constraints, max flow per hierarchy level, edge subdivision, flip fixing through the hierarchy, a sharp-aware and a fixed-vertex position solve, quad extraction, valence and hole fixing, and a final dynamic position solve.
- Two of those solves, `optimize_positions_fixed` and `optimize_positions_dynamic`, factor a sparse symmetric system with Eigen's `SimplicialLLT`. So the port needs its own sparse solver on the default path.
- Upstream picks its max-flow solver by supply: its own `ECMaxFlowHelper` below 20 units, Boost Boykov-Kolmogorov at 20 and above, Lemon network simplex only with `-mcf`. `ECMaxFlowHelper` augments one unit per breadth-first search. Using it for every level, as ADR 0004 decides, gives the same flow value as upstream but not necessarily the same flow, so the port's output is compared with upstream's by value and mesh statistics, not vertex for vertex. How slow one-unit augmentation is on a heavy model is unmeasured.
- Off the default path, and not needed for a faithful default run: `-sharp`, `-boundary`, `-adaptive`, `-mcf` (Lemon), `-sat` (writes a CNF file and runs an external SAT solver), CUDA, TBB, Gurobi, `post-solver.cpp` (Ceres, its call commented out), `merge_close` (commented out), serialisation and the OBJ loader.
- QuadriFlow emits no correspondence. ADR 0001 already settles that Route B builds its map by matching the two meshes geometrically.

Decided from that: the port covers the default run only, as `skp-retopo/src/route_b/`, one module per upstream concern, with the sparse solver and max flow kept inside it. Built in this order, one commit each, each tested before the next:

1. `field_math.rs`, `pcg32.rs`, `dset.rs`
2. `dedge.rs`, `adjacency.rs`
3. `subdivide.rs`
4. `hierarchy.rs`
5. `orient.rs`, orientation field and its singularities
6. `position.rs`, position field and its singularities
7. `sparse.rs`, the replacement for `SimplicialLLT`
8. `flow.rs` (`ECMaxFlowHelper`) and `integer.rs` (edge info, integer constraints, max flow)
9. `flip.rs` (edge hierarchy, flip fixing), and the edge-difference subdivision in `subdivide.rs`
10. `solve.rs`, the fixed position solve
11. `extract.rs` (quad extraction, hole fixing), `valence.rs`, and the dynamic position solve in `solve.rs`
12. `correspond.rs`, and `mod.rs` wired into `skpforge-cli retopo` with cancel and elapsed time
13. Output compared with upstream's on one model

Step 1 is built (`field_math.rs`, `pcg32.rs`, `dset.rs`). pcg32 is checked against upstream's own header compiled with MSVC. What turned up:

- Upstream's `pcg32::shuffle` begins with `if (begin <= end) return;`, so it never shuffles. TBB is off by default in upstream's CMake, so the default build takes the serial graph colouring, whose "random" permutation is therefore the identity: vertices are coloured in index order. The port colours in index order and does not port the shuffle.
- The randomness that does run comes from C `rand()`, for the initial orientation and position of every vertex in `Hierarchy::Initialize`, and from `std::mt19937` with `std::shuffle` in the integer constraints. `rand()` differs between C libraries and `std::shuffle` between standard libraries, so upstream's own output differs between a Linux and a Windows build. Together with the max-flow solver choice, that rules out a vertex-for-vertex comparison in step 13 on any platform. The port draws those numbers from pcg32 instead, so a run is the same on every platform; how its seed is exposed is settled at step 4, where it is first used.
- `dset.hpp` (the lock-free `DisjointSets`) is included but never used. Only `disajoint-tree.hpp` is ported.

Step 2 is built (`dedge.rs`, `adjacency.rs`). Found while doing it: upstream's `compute_direct_graph` returns `true` before its code that splits non-manifold vertices, so that code never runs and the `while (!compute_direct_graph(...))` loops around it run once. A vertex on an edge shared by three or more faces is only flagged; it loses its vertex-to-edge link and gets no neighbours in the adjacency, so the orientation and position fields never reach it. `remove_nonmanifold` is never called. The port does the same. This matters for SketchUp input: `Casa Neoclasica.skp` has 1,334 non-manifold edges after REPAIR, the T-junctions of single-sheet walls. Such models route to A today, but a model that reached Route B with them would have every vertex on those edges left out of the field.

Step 3 is built (`subdivide.rs`). Two findings:

- Upstream writes a new vertex's density as `0.5f * (rho[v0], rho[v1])`, a C++ comma expression that evaluates to half of `rho[v1]`, not the mean. The port does the same. In the default run every `rho` starts at 1 and is only read inside this subdivision, so the effect is limited to how far edges are split.
- The split test compares squared edge length with `rho` directly, and `rho` starts at 1. That 1 is in upstream's normalised units: `Load` recentres the mesh and divides by half its largest bounding-box side, so the model spans [-1, 1] on that axis. The port has to normalise the same way before subdividing or the test means something different at SketchUp's scale, and has to undo it on output. That goes into `mod.rs` with the rest of `Parametrizer::Initialize`.

Step 4 is built (`hierarchy.rs`, and `Parametrizer` in `mod.rs` with loading, normalisation, mesh status, smooth normals, vertex areas and `initialize`). Upstream's OBJ loader numbers vertices by first use in the face list and drops unused positions; the port loads triangles the same way, since vertex order decides the colouring and the order of the random draws. Those draws come from a `Pcg32` passed in by the caller. How its seed is exposed is left to step 12, where the CLI is wired; nothing before then needs it.

Step 5 is built (`orient.rs`). On a cube subdivided to about 300 faces the singular faces add up to eight quarter turns, as Poincaré-Hopf requires of a 4-RoSy field on a sphere-like surface, and a flat patch has none. Upstream's constraint branch is skipped because its weights are empty unless `-boundary` is given.

Step 6 is built (`position.rs`). On a flat patch every pair of neighbours lands on the same lattice to within a millionth of a cell and there are no position singularities. The default run calls `optimize_scale` non-adaptively, which sets every per-vertex scale factor `S` to 1 on every level, and `main.cpp` then switches the adaptive flag on, so every later solve multiplies by those ones and swaps equal values. `K` is only read by the adaptive path. The port leaves both out, which changes no arithmetic.

Decided before step 7: `sparse.rs` is a direct sparse Cholesky (LLᵀ) written here, with its own minimum-degree ordering written from the published algorithm rather than from Eigen's AMD code. Upstream builds each system with both triangles and `SimplicialLLT` reads only the lower one; the port does the same. Upstream never checks whether the factorisation succeeded, and `optimize_positions_fixed` then keeps the old value wherever the solve returned NaN. The port reports a non-positive pivot as an error, and step 10 decides how the solves handle it. Conjugate gradient was considered and not taken: upstream tried it and commented it out, and it answers differently on the singular systems where upstream gets NaN. Whether minimum degree is fast enough on a heavy model is measured at step 12.

Step 7 is built (`sparse.rs`). A one-off timing in a release build, not committed as a test, on grid Laplacians: 10,000 unknowns order in 0.09 s and factor in 0.02 s; 40,000 in 0.81 s and 0.13 s; 90,000 in 3.1 s and 0.5 s. Ordering dominates and grows roughly as n^1.9 because it keeps the explicit elimination graph. The fixed-position solve has two unknowns per vertex group of the subdivided mesh, so a heavy model could spend minutes ordering. If step 12 shows that, the ordering moves to a quotient graph, which gives the same kind of order faster.

Step 8 is built (`flow.rs`, `integer.rs`). On the flat patch and the cube the flow reaches the supply and every face's integer offsets close to zero. `ComputeMaxFlow` builds its edge graph with `DownsampleEdgeGraph(..., 1)`, a single level, so upstream's max flow runs on the finest level only and the per-level loop in `optimize_integer_constraints` runs once. The multi-level edge graph is built only by `FixFlipHierarchy`, so it moves to step 9 with `flip.rs`. The default run marks no sharp edges, so `allow_changes` is all ones and the sharp branches are not ported.

Step 9 is built: the edge-difference split in `subdivide.rs`, and `flip.rs` with the multi-level edge graph and flip fixing. Neither does anything on the flat patch or the cube. Initial subdivision keeps every edge under half a cell, so no offset reaches 2, and max flow leaves no face flipped. Each is checked on a hand-built case instead: a 2 by 2 square whose edges span two cells, and a fan with one flipped face, which the shrink unflips exactly as worked out by hand. Upstream's `FixFlip` calls itself each time it accepts a move; the port runs the same sequence as a loop. Upstream's split queue orders entries by largest offset only, so ties leave in whatever order its standard library gives; the port takes them first in, first out. Where upstream would loop forever, read outside a face, or exit the process on a broken invariant, the port returns `RouteBInvariant`.

Decided before step 10: the fixed and dynamic position solves get a small pull toward the current values. Both build their least-squares systems only from offsets between neighbouring vertex groups, so moving every group by the same tangent translation changes nothing. On a flat component the matrix is exactly singular, and on a curved one it is close. Upstream never checks. Whether the last pivot rounds to a tiny positive or a tiny negative number is an accident of ordering and platform. The fixed solve keeps its old values wherever that gives NaN, and the dynamic solve has no check at all, so NaN can reach the output quads. The port adds eps times (x minus its current value) squared to each system, with eps 1e-8 of the mean diagonal. That keeps the system positive definite on every platform, moves a well-posed answer by about eps, and on a singular one picks the solution nearest the current positions, which is what upstream's fallback keeps. Keeping old values on failure and pinning one vertex per component were considered and not taken.

The sharp solve does nothing in the default run: with no sharp edges it finds no sharp vertices and returns having built nothing. It is not ported. The dynamic solve reads the quad mesh that extraction builds, so it moves to step 11 with `extract.rs`, and step 10 is the fixed solve alone.

Step 10 is built (`solve.rs`). On the flat patch, after max flow, edge split and flip fixing, every pair of neighbouring vertex groups comes out its integer offset apart to within 1e-4 of a cell, and the cube solves to finite positions everywhere.

Step 11 is built (`extract.rs`, `valence.rs`, and the dynamic solve in `solve.rs`). A flat patch with a target of 200 quads comes out as 198, and valence fixing leaves them alone. A cube with a target of 300 comes out as 294 quads on 296 vertices, closed, with 296 minus 588 plus 294 equal to 2, the Euler characteristic of a sphere. After the dynamic solve every cube vertex lies within 0.05 of the cube's surface in normalised units, and mean edge length is within 20 percent of the target cell on both. Upstream behaviour found here and kept:

- `FixHoles` fills every boundary loop shorter than 25 edges, the mesh's own border included. A cap runs opposite to the existing edges, so its duplicate-edge check lets it through, and a small open border gets a lid. On SketchUp input this would close any opening under 25 quad edges.
- It also returns at the first empty sub-loop, which silently skips any holes after it in that loop.
- `FixValence`'s border branch never runs, because its ring walk leaves the edge index valid when it reaches a border. Border vertices take the interior split.
- The dynamic solve discards the result of `axis.normalized()`, so Eigen's angle-axis matrix gets an axis of length sin θ and hardly turns the target offsets. The port reproduces Eigen's formula with that axis.
- Angles are converted with 3.141592654, not π.

Two changes from upstream. Where upstream reads past a bookkeeping array for a vertex added in the same pass, the port grows the array. Where it divides by the ring size of a quad vertex with no ring, which gives NaN, the port skips the vertex.

Decided before step 12, since upstream only writes positions and quads to an OBJ:

- `--target-tris n` asks QuadriFlow for n / 2 quads, so the LOW triangle count lands near the budget.
- The seed is fixed, like upstream's default of 0, with no flag. A run repeats on every platform.
- Each LOW quad takes the front and back material most of its corresponding HIGH triangles carry. Corner normals come from the field. UVQ stays zero, because Phase 4 reprojects UV0 through the correspondence map.
- The correspondence is geometric, as ADR 0001 describes. Each HIGH triangle maps to the LOW triangle nearest its centroid, found through a uniform grid, and any LOW triangle left empty gets the HIGH triangle nearest its own centroid.

Step 12 is built (`correspond.rs`, `route_b` in `mod.rs`, and `skpforge-cli retopo` running B). The first real model exposed two upstream defects that stop it on SketchUp input. Both are fixed, as decided at the time:

- `compute_direct_graph` pairs every copy of a repeated half-edge with the one opposite edge, each pairing overwriting the last, so twin links stop being mutual on non-manifold edges: 382 of 15,171 half-edges on `Casa Neoclasica.skp`. Upstream's edge split then loops forever. The port pairs a half-edge only when its own direction and the opposite direction each occur once.
- Edges at non-manifold vertices are never queued for splitting and the field never reaches those vertices, so their offsets stay arbitrary and upstream prints "wrong init" and exits with status 0 and no output. Upstream's own code to split such vertices sits after an unconditional return. The port runs it, rebuilding the graph until nothing splits, so a T-junction becomes separate open sheets. That dead code would also have indexed with -1 at a border and never grew `rho`; the port walks each fan both ways and copies `rho`.

With both fixes, forced onto Route B with `--target-tris 2000`, release build:

| Model | Quads asked | Quads made | LOW triangles | Integer flow | Pairs | Time |
|---|---|---|---|---|---|---|
| `Casa Neoclasica.skp` | 1,000 | 952 | 1,904 | 127 of 127, 1 round | 6,424 | 1.22 s |
| `Estación de Salamanca.skp` | 1,000 | 844 | 1,688 | 308 of 308, 1 round | 6,381 | 4.36 s |

On `3d66.com_1154175.skp` at `--target-tris 100000`, also forced onto B, it asks for 50,000 quads and makes 44,136 (88,272 LOW triangles, 11,728 under budget) with a valid map, in 441 s. Per stage: initialise 16 s, orientation field 18 s, position field 40 s, integer offsets 246 s, edge split and flips 8 s, fixed solve 5 s, quad extraction 32 s, valence 0.1 s, dynamic solve 27 s, correspondence 46 s. A second run with timing prints, not committed, broke the integer stage down: orientation tree and components 2.1 s, first balancing 0.3 s, and the first max-flow round 203.5 s to push 3,725 of 3,726 units. The second round pushed the last unit at once. So the risk recorded when upstream was read is real: `ECMaxFlowHelper` augments one unit per breadth-first search, and on a heavy model it is most of the run. The sparse solver is not a problem at this size; the fixed solve, ordering included, takes 5 s. Speeding up the max flow is a change of solver or of how it augments, and needs a decision.

Cancel is checked inside the long loops too: before every max-flow augmentation, before every round of the dynamic solve, and every 4,096 HIGH triangles in the correspondence.

Step 13 is done. Upstream was built from `810b7a0` in a throwaway directory outside the repository, with Boost 1.84 and Eigen 3.4 headers downloaded there and nothing committed, using MSVC with `/O2` and upstream's default options. Upstream's own release flag is `-O3`, which MSVC ignores, so without that override it builds unoptimised. The port ran through `route_b_obj`, and both outputs were measured with `tools/route-b-compare/stats.py` on tori from `tools/route-b-compare/torus.py`, since upstream cannot read `.skp`:

| Input | Target quads | Port | Upstream |
|---|---|---|---|
| Torus 120 by 60, 14,400 triangles | 1,000 | 994 quads, 11 valence-3 and 11 valence-5, 0.60 s | 1,056 quads, 11 and 11, 0.73 s |
| Torus 400 by 200, 160,000 triangles | 10,000 | 9,271 quads, 9 and 9, 18.4 s | 8,903 quads, 8 and 8, 11.6 s |

Every output is closed with Euler characteristic 0, as a torus must be, and has no edge shared by three faces. The counts agree to within 7 percent and the singularities to within one pair. The port's outputs are not expected to match vertex for vertex, as recorded when upstream was read. On the larger torus the port is slower, and 5.8 s of its 18.4 s are the integer stage, where upstream uses Boykov-Kolmogorov. Upstream on the repaired `Casa Neoclasica.skp` hung in "Solve index map" until a 600 s timeout killed it, and wrote nothing, which is the infinite loop the twin-link fix removes.

ADR 0005 replaced that max flow with Dinic's algorithm. Measured afterwards, it is slower here, with identical output. On the 160,000-triangle torus the integer stage took 5.8 s with upstream's one-unit solver, 18.6 s with Dinic, and 11.6 s once Dinic's level search stops at the sink's level. On `3d66.com_1154175.skp` it took 246 s with the one-unit solver and 302 s with Dinic before that cut-off. A probe on the torus, not committed, showed why: 145 phases for 174 units, with the sink's level rising almost every phase (3, 6, 7, 10, 15, and on), 4.8 s in the level searches and 6.6 s in the blocking-flow walks. The imbalances in this network are few and far apart, so nearly every shortest path is a little longer than the last and each phase carries one path. The one-unit solver stops each search at the first sink it reaches, which suits that shape better. ADR 0005's premise, that one phase would push many paths, does not hold on this network.

ADR 0006 replaced Dinic with Boykov-Kolmogorov, which is also what upstream runs for every supply of 20 or more. It keeps its search trees between augmentations, which suits scattered imbalances. The same flow, measured:

| Input | One-unit solver | Dinic | Boykov-Kolmogorov |
|---|---|---|---|
| Torus 400 by 200, integer stage | 5.8 s | 11.6 s | 2.0 s |
| `3d66.com_1154175.skp`, integer stage | 246 s | 302 s | 13.6 s |
| `3d66.com_1154175.skp`, whole Route B | 441 s | 460 s | 137 s |

With Boykov-Kolmogorov the heavy model makes 44,024 quads and the big torus 8,343, against 44,136 and 9,271 before. The flow value is the same, but which flow is found decides the quads. The torus is still closed with Euler characteristic 0 and 9 valence-3 and 9 valence-5 vertices, against upstream's 8,903 quads with 8 and 8. The heavy model's time is now spread over the position field (32 s), quad extraction (29 s) and the dynamic solve (21 s), with no single stage dominating.

Route B is done as a port. Still open in this phase: no organic model has been run, so the exit criterion that a heavy organic model routes to B is untested.

All three are CAD models that ROUTE sends to A; they were forced onto B because they are the models there are. Whether the quads look right has not been checked visually. Pairs exceed the HIGH triangle count because every LOW triangle no HIGH centroid reached also gets its nearest HIGH triangle.

- [x] **Correspondence map emitted by both routes**, as a first-class output
- [x] Correspondence validated: every LOW triangle maps to at least one HIGH triangle. Route A validates its composed map, and Route B validates its geometric one before returning it

Exit: a CAD-like model over budget routes to A with its corners intact; a heavy organic model routes to B; both emit a valid correspondence map.

Related: [ADR 0003](docs/adr/0003-route-b-quadriflow-as-a-bundled-sidecar.md) chose to vendor QuadriFlow and run it as a sidecar; reading upstream showed it needs Boost Graph and Eigen, and [ADR 0004](docs/adr/0004-route-b-quadriflow-ported-to-rust.md) supersedes it with a Rust port that uses QuadriFlow's own max-flow solver. The correspondence map representation is settled in [ADR 0001](docs/adr/0001-correspondence-map-representation.md).

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
