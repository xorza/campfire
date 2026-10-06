# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

B12's harness is as [A client's re-simulated 3v3 tick](docs/design/13-benches.md#a-clients-re-simulated-3v3-tick) proposes it, under "How the harness plays it".

1. **B12b. Each client's time and a walk around a point** (net): a timed step keeps each client's frame time in a buffer the match keeps; `walk_steps` takes the point the avatar walks around. Check: `client_frame/walk`, `walk_rollback` and `worst_1v1` and `server_frame/*` measure as the record says, within their spread.
2. **B12c. The 3v3 client cases** (net): the fixture of the reference 3v3 at 30 Hz, two players and four bots picking by `[[input]]`s, stepped to tick 3,600; `client_frame/walk_3v3` and `client_frame/walk_rollback_3v3`; a re-simulated tick from their difference and `PredictionMetrics`' `rollback_ticks` over `rollbacks`; the suite's rows, the record's figures, and design 02's bound of an 8-tick rollback from the figure. Check: the fixture's time, and the bench step's in CI, against its limit of 60 s; past it, stop for review.
