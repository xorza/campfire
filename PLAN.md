# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

Steps B7 to B9 wait for review: [PLAN_QUESTIONS.md](PLAN_QUESTIONS.md#kernels-for-the-new-hot-stages).

1. **B7. The Inputs stage's worst** (B6 of [Benches](docs/design/13-benches.md#record)): it takes 1.66 ms of the 3.13 ms worst 3v3 tick, against 6.9 µs on average. Profile the ticks where it peaks, name the path, and give it a kernel and its capability's Cost figure.
2. **B8. The Move stage's worst** (the same): 0.90 ms of the worst tick, against 11.5 µs on average. The same work.
3. **B9. The Resolve stage** (the same): 20.0 µs, 13 % of the mean tick. The same work.
