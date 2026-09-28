# 0003 — Route B obtains QuadriFlow as a bundled sidecar

Status: Superseded by 0004

Date: 2026-09-28

Deciders: Felipe Carvajal Brown

## Context

Route B sends heavy organic input to a field-aligned remesher, and QuadriFlow is
the one named for it. Its licence is permissive, so bundling it is allowed. The
question is how skpforge gets a working QuadriFlow at run time.

Two project rules bear on it. Nothing comes from crates.io, and C or C++ code
enters only as source vendored under `vendor/<name>/`, read before it is
committed, and compiled through `cc`, as `xatlas` is. And anything long-running
takes a cancellation token: a remesher reports no progress, so the UI shows
elapsed time and a cancel that actually stops the work.

A solve inside a library call cannot be interrupted from outside without
changing the library. A child process can be killed.

## Decision

QuadriFlow's source is vendored under `vendor/quadriflow/`, read and committed,
and compiled through `cc`. It is linked into a small helper executable that
ships with skpforge, and Route B runs it as a child process.

Cancelling Route B kills that process. Its progress is elapsed time only.

Whatever QuadriFlow itself depends on is established when its source is read,
and each of those dependencies is vendored and read under the same rule before
anything is committed.

## Consequences

- Route B works on every install, with no separate download or path to set.
- Every line that runs is in the repository and has been read.
- Cancel is real: killing the process stops the solve at any point.
- QuadriFlow's own dependencies add to what has to be vendored and read, and
  their size is not an objection under the dependency rule.
- The mesh crosses a process boundary both ways, so skpforge writes the HIGH
  input for the helper and reads its output back. The format for that exchange
  is not decided here.
- The helper is a second executable to sign, since an unsigned Windows binary
  that spawns child processes is flagged by enterprise antivirus.

## Alternatives considered

- **Vendored and linked in-process.** The same source, called through FFI. Less
  to wire, but a running solve cannot be cancelled without patching QuadriFlow.
- **User-installed sidecar.** Nothing vendored; the user supplies a binary, as
  with `quadwild-bimdf`. No build cost, but the remesher is unread code, and
  Route B works only where someone has installed it.
