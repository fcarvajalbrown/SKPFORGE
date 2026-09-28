# 0002 — Weld tolerance

Status: Accepted

Date: 2026-09-28

Deciders: Felipe Carvajal Brown

## Context

REPAIR welds vertices first, and everything after it depends on the result.
ROUTE reads its density metrics off the welded mesh, and DISPLACE relies on
welding to make a vertex shared between the faces that meet at it. Two
vertices within the tolerance become one; two outside it stay apart.

IMPORT hands REPAIR a mesh whose coordinates went through the inches to `Uu`
conversion and through every nested group and component transform on the way.
Points that were one vertex in SketchUp can arrive a hair apart. A tolerance
that is too tight leaves them unwelded; one that is too loose collapses real
detail.

SketchUp stores every model in inches internally, whatever display units the
modeller works in, and merges vertices closer than 0.001 inch. A `.skp`
therefore holds no geometry finer than that which carries meaning.

`skp-core` already carried `DEFAULT_WELD_TOLERANCE = Uu(0.001)`, 0.01 mm, with
no recorded reason for the value.

## Decision

The default weld tolerance is SketchUp's own: 0.001 inch, which is 0.00254 `Uu`
(0.0254 mm). It is fixed, not derived from the model, and it is expressed
through `units.rs` rather than as a second literal.

A `--weld-tolerance` override lets a run use a different value.

## Consequences

- Vertices SketchUp itself treated as one are welded as one, and vertices
  SketchUp kept apart stay apart.
- The tolerance is the same for a small prop and for a large site model.
- The existing 0.01 mm constant is replaced.
- The display units a model was drawn in have no effect.

## Alternatives considered

- **Keep 0.01 mm.** Tighter than SketchUp, so safer against merging detail, but
  float drift through nested component transforms can leave vertices that
  should join unwelded.
- **Scale to the model's size**, as a fraction of the bounding-box diagonal. On
  large site models it grows coarse enough to collapse small real detail such
  as window mullions.
- **Derive from the shortest edge.** A single sliver face anywhere in the model
  shrinks the tolerance for all of it.
