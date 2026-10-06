# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

The kernels of B8 to B10 wait for review: [PLAN_QUESTIONS.md](PLAN_QUESTIONS.md#kernels-for-the-hot-stages-b8-to-b10).

1. **B8. The pathing grid's kernel** (capabilities; [Hot stages](docs/design/13-benches.md#hot-stages), Inputs): `pathing_grid/build` and `pathing_grid/one`; navigation's Cost section names them, with their figures.
2. **B9. The route planner's kernel** (capabilities; the same, Move): `route_planner/crowded` and `route_planner/spread`; navigation's Cost section names them, with their figures.
3. **B10. The script view's kernel** (capabilities; the same, Resolve): `script_view/read`; design 02's scripting section names it, with its figure.
