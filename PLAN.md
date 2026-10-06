# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

1. **B7. The stages' closing sets** (sim, capabilities, runner; [B5](docs/design/13-benches.md#decisions)): `sim`'s `SimEdge` sets, `Start` and `After` of each stage; `stats`' refresh passes in them; `StageClock`'s probes after them, the first after `start_tick` and the last before `end_tick`; the [record](docs/design/13-benches.md#record)'s `server_stage` figures and its stages past 10 % measured again. Check: the stage test with a system in a closing set, timed within the stage before it; a test that fails a system of the proving match's schedule in no stage and no edge set; the nine means add up to less than `server_tick/mean_3v3`.
2. **B8. The Inputs stage's worst** (B6 of [Benches](docs/design/13-benches.md#record)): 1.66 ms of the 3.13 ms worst 3v3 tick before B7, against 6.9 µs on average. Profile the ticks where it peaks, name the path and the scene its kernel needs, and write that into design 13 for review; then the kernel and its capability's Cost figure.
3. **B9. The Move stage's worst** (the same): 0.90 ms of the worst tick before B7, against 11.5 µs on average. The same work.
4. **B10. The Resolve stage** (the same): 20.0 µs before B7, 13 % of the mean tick. The same work.

Steps B8 to B10 start from B7's figures: a stage that no longer passes 10 % of a tick has its step removed, and a new one gets a step.
