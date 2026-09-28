# 0006 — Route B solves its max flow with Boykov-Kolmogorov

Status: Accepted. Supersedes 0005

Date: 2026-09-28

Deciders: Felipe Carvajal Brown

## Context

ADR 0005 replaced QuadriFlow's one-unit max-flow solver with Dinic's
algorithm, on the premise that each Dinic phase would push many augmenting
paths at once. Measured afterwards on the same output, it was slower. On a
160,000-triangle torus the integer stage took 5.8 s with the one-unit solver,
18.6 s with Dinic, and 11.6 s with Dinic's level search stopped at the sink's
level. On `3d66.com_1154175.skp` it took 246 s with the one-unit solver and
302 s with Dinic.

A probe showed why: 145 phases for 174 units on the torus, the sink's level
rising almost every phase. The imbalances in this network are few and far
apart, so nearly every shortest path is slightly longer than the last, and a
phase carries one path.

For every supply of 20 units or more, upstream QuadriFlow does not use its own
solver at all. It uses Boost's Boykov-Kolmogorov solver, an algorithm built for
grid-like graphs with scattered sources and sinks, which keeps its search trees
between augmentations instead of searching again from scratch.

## Decision

Route B remains a Rust port of QuadriFlow inside `skp-retopo`, with nothing
from QuadriFlow, Eigen, Boost or Lemon vendored, as ADRs 0004 and 0005 decided.
Its max flow is the Boykov-Kolmogorov algorithm, written here from its
published description and not from Boost's code. The network, the capacities,
the rounds and the way a finished flow is applied to the integer offsets are
unchanged.

## Consequences

- The flow value is unchanged, since every correct max-flow algorithm finds the
  same maximum.
- For supplies of 20 or more the port now runs the same algorithm upstream
  runs, which brings it closer to upstream than either earlier choice. The
  exact flow can still differ, because tie-breaking follows this code and not
  Boost's.
- Whether it is faster than the one-unit solver on this network is measured
  after it is built; ADR 0005 showed that a better complexity bound does not
  settle that here.
- Dinic's algorithm is removed.

## Alternatives considered

- **Return to QuadriFlow's one-unit solver.** The fastest measured so far, and
  faithful to upstream below 20 units, but the heavy model stays around four
  minutes in this stage.
- **Keep Dinic (ADR 0005).** Slower than the one-unit solver on both measured
  inputs.
