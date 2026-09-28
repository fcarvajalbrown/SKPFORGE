# 0005 — Route B solves its max flow with Dinic's algorithm

Status: Superseded by 0006

Date: 2026-09-28

Deciders: Felipe Carvajal Brown

## Context

ADR 0004 made Route B a Rust port of QuadriFlow with nothing vendored, and
chose QuadriFlow's own max-flow solver, `ECMaxFlowHelper`, where upstream
offers several. That solver augments one unit of flow per breadth-first search.

Upstream itself only uses it when the supply is below 20 units. At 20 and
above it hands the problem to Boost's Boykov-Kolmogorov solver. So the exact
flow the port finds never matched upstream's on any model large enough to
matter; only the flow value does.

Measured on the port: on `3d66.com_1154175.skp` at a budget of 100,000
triangles, the first max-flow round has a supply of 3,726 units and takes
203.5 s of a 441 s run. On a 160,000-triangle torus the port takes 18.4 s to
upstream's 11.6 s, 5.8 s of it in the integer stage.

## Decision

Route B remains a Rust port of QuadriFlow inside `skp-retopo`, with nothing
from QuadriFlow, Eigen, Boost or Lemon vendored, as ADR 0004 decided. Its max
flow is Dinic's algorithm, written here from the published algorithm, in place
of `ECMaxFlowHelper`. The network, the capacities, the rounds and the way a
finished flow is applied to the integer offsets are unchanged.

## Consequences

- The flow value is the same as before and as upstream's, because every
  correct max-flow algorithm finds the same maximum. Which flow is found can
  differ, as it already did from upstream above 20 units of supply.
- Each phase of Dinic's algorithm pushes a blocking flow along every shortest
  path at once, so the number of searches no longer grows with the supply.
- `ECMaxFlowHelper` is no longer ported code; the port's max flow has no
  upstream counterpart to read against, and is checked by its own tests and by
  the flow value it reaches.

## Alternatives considered

- **Keep `ECMaxFlowHelper` (ADR 0004 as written).** Faithful to upstream's
  small-supply solver, and Route B still works, but a heavy model spends most
  of its run in this stage.
- **Keep the same search but push the path's bottleneck capacity.** The
  smallest change, but many paths in this network have a bottleneck of one
  unit, so the gain was uncertain before measuring.
