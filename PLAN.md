# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Benches](docs/design/13-benches.md): three tiers, one rule for a case's id, a case for each stage of the tick, and a kernel for each path past 10 % of a tick. Each step ends with `-- --list` printing its ids, and the check chain with `cargo test --bench '*'` passing for its crates.

1. **B10. The script view's kernel** (capabilities; [Hot stages](docs/design/13-benches.md#hot-stages), Resolve): `script_view/read` and `script_view/reread`; the group joins [B2](docs/design/13-benches.md#decisions)'s list and its cases [the suite](docs/design/13-benches.md#the-suite); design 02's scripting section names them, with their figures.
2. **B11. The mean cases over whole matches** (runner; [Record](docs/design/13-benches.md#record), "The mean cases"): `server_tick/mean_3v3` and the nine `server_stage/mean_3v3_<stage>` play one whole match an iteration and state its 6,000 ticks as their throughput; the record's mean rows measured again, and design 13's Cost in CI with the step's new time. Check: the nine stage means add up to less than the tick's in two runs.
3. **B8. The pathing grid's kernel** (capabilities; Hot stages, Inputs): `pathing_grid/one`; the group and its case join B2 and the suite as B10's do; navigation's Cost section names it, with its figure.
4. **B9. The route planner's kernel** (capabilities; Hot stages, Move): `KernelScene` gains walls that split a scene into lanes and a jungle; `route_planner/walled`, its throughput the planner's units of work; the group and its case join B2 and the suite as B10's do; navigation's Cost section names it, with its figure and the time of a tick's work limit.
5. **B12. A client's re-simulated 3v3 tick** (net; [Bevy](docs/design/02-engine-core.md#bevy), Measured): no case measures it, so design 02 bounds an 8-tick rollback by eight of the server's worst ticks, 26 ms. Propose in design 13 how a case plays a predicting client through the 3v3 and times its re-simulated ticks, for review; then the case, and design 02's bound from its figure.
