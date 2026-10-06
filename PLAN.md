# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

1. **B8. The pathing grid's kernel** (capabilities; [Hot stages](docs/design/13-benches.md#hot-stages), Inputs): `pathing_grid/one`; the group joins [B2](docs/design/13-benches.md#decisions)'s list and its case [the suite](docs/design/13-benches.md#the-suite); navigation's Cost section names it, with its figure.
2. **B9. The route planner's kernel** (capabilities; Hot stages, Move): `KernelScene` gains walls that split a scene into lanes and a jungle; `route_planner/walled`, its throughput the planner's units of work; the group and its case join B2 and the suite as B8's do; navigation's Cost section names it, with its figure and the time of a tick's work limit.
3. **B12. A client's re-simulated 3v3 tick** (net; [Bevy](docs/design/02-engine-core.md#bevy), Measured): no case measures it, so design 02 bounds an 8-tick rollback by eight of the server's worst ticks after the first, 10 ms. Propose in design 13 how a case plays a predicting client through the 3v3 and times its re-simulated ticks, for review; then the case, and design 02's bound from its figure.

## Core

1. **C1. The match end stops the gaps** (capabilities; [Game scripting](docs/design/03-game-scripting.md), End): `match_end` gives `SimEdge::Start` and each `SimEdge::After` the `MatchEnd::running` condition its stages have. Check: a test ends a match and finds that no refresh pass and no `keep_in_bounds` runs in the ticks after, and the proving match's goldens hold.
