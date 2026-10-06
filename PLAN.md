# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

B12's case waits for review: [PLAN_QUESTIONS.md](PLAN_QUESTIONS.md#a-clients-re-simulated-3v3-tick-b12).

1. **B12. A client's re-simulated 3v3 tick** (net; [A client's re-simulated 3v3 tick](docs/design/13-benches.md#a-clients-re-simulated-3v3-tick)): the case of the reviewed option; design 02's bound of an 8-tick rollback from its figure, in place of eight of the server's worst ticks after the first, 10 ms.

## Core

1. **C1. The match end stops the gaps** (capabilities; [Game scripting](docs/design/03-game-scripting.md), End): `match_end` gives `SimEdge::Start` and each `SimEdge::After` the `MatchEnd::running` condition its stages have. Check: a test ends a match and finds that no refresh pass and no `keep_in_bounds` runs in the ticks after, and the proving match's goldens hold.
