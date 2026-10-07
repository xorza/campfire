# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## SIMD

The design is [SIMD](docs/design/11-simd.md): 256-bit lanes in `math`, the same on x86-64-v3 and armv8-a, and the grid's rows on them. Each step measures its cases before and after, in one run on one core, and ends with the check chain passing for its crate.

1. **S2. The grid's rows** (core; D8, D9): `Grid::spans` takes four rows a step behind its gate, and its rows one by one past it. Check: the grid's span tests, a property of the lanes against the scalar path with the gate's edges, the proving match's goldens and the golden digests unchanged; `fog/sight`, `pathing_grid/one` and `server_tick/mean_3v3` before and after; vision's Cost section; CI's macOS job, on arm64, green.
