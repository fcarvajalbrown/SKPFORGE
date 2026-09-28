I am building skpforge, a tool that turns SketchUp models into assets for Unreal Engine 5. One of its stages is retopology, and one of its two retopology routes is a field-aligned quad remesher for heavy, organic models. The obvious choice for that route is QuadriFlow. The project has one hard rule that got in the way: nothing comes from crates.io, and any C or C++ library has to be vendored and read before it is committed.

QuadriFlow's build needs Boost Graph and Eigen. I could not honestly claim to have read either. So I ported QuadriFlow to Rust instead.

## What I ported

I read upstream's `src/` at commit `810b7a0`, 9,232 lines, and ported only what its default run executes: `quadriflow -i in.obj -o out.obj -f <faces>`. That left out the flags for sharp edges, boundaries, adaptive scale, min-cost flow and SAT, plus CUDA and TBB. What remained is a pipeline of about a dozen stages: subdivide, build a multi-resolution hierarchy, solve an orientation field, solve a position field, turn it into integer edge offsets with a max flow, fix flipped faces, solve positions, extract quads, fix valences, solve positions again.

The port is one Rust module per upstream concern, built bottom up, each piece tested before the next depended on it.

## Things upstream does that you only see by reading it

Some of what the default run does is not what the code appears to do.

The graph colouring shuffles its vertices with pcg32 first. The shuffle begins with `if (begin <= end) return;`, so it never shuffles. With TBB off, which is the default build, vertices are coloured in index order.

When an edge is split, the new vertex's density is `0.5f * (rho[v0], rho[v1])`. That is a C++ comma expression. It evaluates to half of `rho[v1]`, not the mean.

`compute_direct_graph` has code to split non-manifold vertices. It sits after an unconditional `return true;`, so it never runs.

The other randomness comes from C `rand()` and `std::shuffle`, which differ between standard libraries. So upstream's own output differs between a Linux and a Windows build, and I stopped expecting a vertex-for-vertex match. The port uses pcg32 for everything and gives the same result on every platform.

I kept all of these, because the goal was a faithful port first.

## Two bugs that stop it on SketchUp input

SketchUp models are full of T-junctions: a wall whose edge meets another wall's face. After cleanup, one of my test models, a neoclassical house, still had 1,334 edges shared by three or more faces.

The first bug is in how upstream pairs half-edges. On such an edge, every copy of a repeated half-edge pairs with the single opposite one, and each pairing overwrites the last. The twin links stop being mutual: on that house, 382 of 15,171 half-edges pointed at a twin that did not point back. Later, the edge-splitting step looks for the rotation between two twins' offsets with `while (rshift90(D01, orient) != Ds10) orient += 1;`. For two unrelated edges no rotation matches, and the loop never ends.

The second bug is the unreachable split. Edges at non-manifold vertices are never queued for splitting, and the fields never reach those vertices, so their offsets stay arbitrary. Upstream then prints "wrong init" and exits with status 0 and no output.

I built upstream separately to check. On that house it sat in "Solve index map" until a 600 second timeout killed it. The port now pairs a half-edge only when both directions are unique, and runs the vertex split upstream never reaches. It finishes the same model in 1.2 seconds.

## Checking the port against upstream

Upstream cannot read SketchUp files, and it cannot handle any of my three test models, so I compared the two on tori, which both accept:

| Input | Target quads | Port | Upstream |
|---|---|---|---|
| Torus, 14,400 triangles | 1,000 | 994 quads, 11 valence-3, 11 valence-5, 0.60 s | 1,056 quads, 11 and 11, 0.73 s |
| Torus, 160,000 triangles | 10,000 | 9,271 quads, 9 and 9, 18.4 s | 8,903 quads, 8 and 8, 11.6 s |

Every output is a closed quad mesh with Euler characteristic 0, as a torus should be. Counts agree within 7 percent and singularities within one pair.

## The max flow

The port was slower on the big torus, and on my heaviest test model the reason was obvious. That model has 662,843 triangles. With a budget of 100,000, one max-flow round had to push 3,726 units, and it took 203.5 seconds of a 441 second run.

That solver is QuadriFlow's own. It pushes one unit per breadth-first search. Upstream only uses it when the supply is below 20 units. At 20 and above it hands the problem to Boost's Boykov-Kolmogorov solver.

My first replacement was Dinic's algorithm, which has the better textbook bound. It was slower: 11.6 seconds on the big torus against 5.8 for the one-unit solver, once I had stopped its level search at the sink's level. A probe showed why. It needed 145 phases for 174 units. The imbalances in this network are few and far apart, so almost every shortest path is a little longer than the last, and each phase carries one path.

Boykov-Kolmogorov keeps its two search trees between augmentations instead of searching from scratch. I wrote it from the paper. On the big torus the integer stage fell to 2.0 seconds. On the heavy model it went from 246 seconds to 13.6, and the whole run from 441 seconds to 137, producing 44,024 quads.

The better textbook bound lost. The algorithm upstream actually runs for this workload won, and I only knew because I measured.

## What is next

All three of my test models are architectural, and the router sends them to the other route, which pairs triangles into quads without moving any corner. The remesher still needs an organic model to show what it is for. After that comes the part skpforge exists for: fixing SketchUp's UVs so a brick wall tiles at the same scale everywhere in Unreal.

The code, the port and the comparison tools are on GitHub: [github.com/fcarvajalbrown/SKPFORGE](https://github.com/fcarvajalbrown/SKPFORGE).
