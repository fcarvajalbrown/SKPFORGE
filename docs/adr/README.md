# Architecture decision records

One file per significant decision, numbered and immutable once accepted. A
decision that changes course gets its own next-numbered record and sets the old
one's status to superseded; an accepted record is never rewritten or extended.

| # | Title | Status |
|---|---|---|
| [0001](0001-correspondence-map-representation.md) | Correspondence map representation | Accepted |
| [0002](0002-weld-tolerance.md) | Weld tolerance | Accepted |
| [0003](0003-route-b-quadriflow-as-a-bundled-sidecar.md) | Route B obtains QuadriFlow as a bundled sidecar | Superseded by 0004 |
| [0004](0004-route-b-quadriflow-ported-to-rust.md) | Route B ports QuadriFlow to Rust | Superseded by 0005 |
| [0005](0005-route-b-max-flow-by-dinic.md) | Route B solves its max flow with Dinic's algorithm | Accepted |
