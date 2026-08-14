# 0001 — Correspondence map representation

Status: Accepted

Date: 2026-08-14

Deciders: Felipe Carvajal Brown

## Context

RETOPO destroys vertex correspondence. Once the LOW mesh exists there is no
implicit way back to the geometry it was derived from, so UV and BAKE cannot
find the original surface unless RETOPO hands them an explicit map. That is why
the map is a first-class output rather than a debug artifact, and why its shape
had to be settled in Phase 0 even though the producer arrives in Phase 3.

The mapping is one-to-many in both directions. A LOW triangle usually covers
many HIGH triangles after decimation, and a single HIGH triangle can straddle
several LOW triangles. Two consumers read it and they read it differently:

- `skp-bake` queries it in an inner loop, once per texel of the output map, to
  narrow the BVH to the HIGH triangles worth intersecting. This is the hot path
  and it dominates bake time.
- `skp-uv` queries it to decide whether a face's UV0 can be reprojected
  analytically or has to be routed to a bake.

Both routes in Phase 3 will produce the map, and they will produce it in
different orders. Route A pairs triangles into quads and decimates, so pairs
emerge grouped by LOW triangle. Route B runs a field-aligned remesher as a
sidecar process and then matches the two meshes geometrically, so pairs emerge
in whatever order the spatial query returns them.

The dependency rule forbids anything from crates.io, so whatever is chosen is
written here, with `std` and nothing else.

## Decision

The map is stored in compressed sparse row form: an offsets array of
`low_triangle_count + 1` entries and a flat run of HIGH triangle indices.

```rust
pub struct Correspondence {
    low_triangle_count: usize,
    offsets: Vec<u32>,
    high: Vec<u32>,
}
```

`high_for(low)` returns a `&[u32]` slice, so a lookup is two array reads and no
allocation. The whole map is two allocations regardless of triangle count.

It is keyed on the LOW **triangle** index after triangulation, matching how the
PRD and CLAUDE.md both word it, not on the LOW face index. A quad face is two
triangles and they can correspond to different parts of the HIGH mesh.

Construction goes through `CorrespondenceBuilder`, which accepts `(low, high)`
pairs in any order, then sorts and deduplicates them at `build()`. A HIGH
triangle either corresponds to a LOW triangle or it does not, so a repeated pair
is one pair, and the same set of pairs always builds the same map regardless of
the order the route emitted them in.

`validate()` enforces the ROADMAP's rule that every LOW triangle maps to at
least one HIGH triangle, and checks the offsets array for internal consistency.

## Consequences

- The bake's inner loop walks contiguous memory. The alternative shapes put a
  pointer chase or a binary search on the hottest path in the pipeline.
- The map is immutable once built. A route that wants to refine its pairing has
  to do it before `build()`, in the builder, or rebuild the map.
- Both routes can emit pairs however it suits them, because ordering is the
  builder's problem rather than theirs.
- Deduplication is silent. A route that emits the same pair twice gets no signal
  that it did.
- The builder's `push` records a pair without checking it. A LOW index beyond
  the declared triangle count therefore panics inside `build()` on the offsets
  array bounds check, rather than surfacing as a typed error. This is a
  programming error in a route, not a data condition, and it is treated the way
  an out-of-bounds slice index is treated.
- `Correspondence` deliberately does not know the HIGH triangle count, so
  `validate()` cannot range-check the HIGH indices it holds. If that check is
  wanted later it needs the count passed in.
- The struct serialises to disk as-is, two arrays and a length, if the map ever
  needs to be written out or cached.

## Alternatives considered

**`Vec<Vec<u32>>`, one row per LOW triangle.** The obvious shape, simplest to
read, and mutable after construction so a route could keep refining the map.
Rejected on allocation behaviour: a 200k-triangle LOW mesh means 200k small heap
allocations scattered across memory, and the bake then chases a pointer per LOW
triangle in the loop that runs once per output texel.

**A single `Vec<(u32, u32)>` sorted by LOW index, range-searched.** One
allocation, and the most natural thing for a remesher to stream out. Rejected
because lookup becomes an O(log n) binary search rather than an O(1) slice, and
because storing the LOW index once per pair instead of once per triangle costs
roughly twice the memory of CSR at the same pair count. The pair list survives
inside the builder, where ordering has to be fixed anyway, and is collapsed to
CSR at `build()`.

**`HashMap<u32, Vec<u32>>`.** Considered and dismissed without detailed
comparison. It carries the per-row allocation cost of the first alternative and
adds hashing to a lookup whose key is already a dense index from zero.
