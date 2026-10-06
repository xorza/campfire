# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

1. **B4. The fog's kernel** (capabilities; [B6](docs/design/13-benches.md#decisions)): the Vision stage's grid fog as a method that `vision::see` and the bench call, as `Broadphase` serves `collide`; one scene builder for every kernel of `capabilities`, which `collision` takes too; `fog/crowded` and `fog/spread`. Vision's Cost section names them, with their figures. Check: vision's tests, unchanged.
2. **B5. The script host's kernel** (script; [B6](docs/design/13-benches.md#decisions)): `script` gets its `bench` feature, `criterion` as its optional dependency, its `[[bench]]` and `lib.rs`'s `bench` facade; `script/call` and `script/native`. Design 02's scripting section names them, with their figures.
3. **B6. The record** ([B6](docs/design/13-benches.md#decisions)): the whole suite measured on one machine, named; each figure the design states names its case id; the stages past 10 % of the mean or the worst tick listed in 13, with a kernel step for each new one.
