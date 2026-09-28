# 0004 — Route B ports QuadriFlow to Rust

Status: Superseded by 0005

Date: 2026-09-28

Deciders: Felipe Carvajal Brown

## Context

ADR 0003 settled that QuadriFlow's source would be vendored, read, compiled
through `cc` and run as a helper process, and that whatever it depends on would
be established when its source was read.

Reading upstream established it. `src/flow.hpp` includes
`boost/graph/adjacency_list.hpp` and the Boost Boykov-Kolmogorov, Edmonds-Karp
and push-relabel max-flow headers unconditionally, and the build requires Boost
and Eigen as external packages. Lemon 1.3.1, pcg32, a parallel stable sort and
the MapleCOMSPS_LRB SAT solver are bundled under `3rd/`. QuadriFlow also carries
its own max-flow solver, `ECMaxFlowHelper`, which uses no external library.

The dependency rule admits C and C++ code only when it can be read before it is
committed. Eigen and the part of Boost Graph that QuadriFlow pulls in cannot
honestly be read in full, and vendoring QuadriFlow with them cut out means
carrying a patched copy of someone else's code. The same rule says rewriting
something that already exists is never an objection.

## Decision

Route B is a Rust port of QuadriFlow inside `skp-retopo`. Nothing from
QuadriFlow, Eigen, Boost or Lemon is vendored. Where QuadriFlow offers several
max-flow solvers, the port uses QuadriFlow's own.

## Consequences

- Every line Route B runs is written and read in this repository.
- There is no C++ build and no second executable for Route B.
- Cancel works through the cancellation token like every other stage, instead
  of by killing a process.
- The port is a large piece of work, and its output has to be checked against
  QuadriFlow's on the same input to know it is faithful.
- Later upstream fixes do not arrive on their own; they have to be read and
  carried over by hand.

## Alternatives considered

- **Vendor QuadriFlow, Eigen and Lemon, and patch Boost out.** Keeps ADR 0003's
  shape, but Eigen is still too large to read, and the vendored QuadriFlow is a
  patched copy.
- **Vendor everything unchanged.** Matches ADR 0003 literally, and is the most
  unread code of any option.
- **User-installed sidecar.** Nothing to build or read, but Route B would run
  unread code and work only where someone has installed QuadriFlow.
