# Campfire — Benches

Proposal: one shape for every bench, by tier, and the benches the hottest paths need now. [Structure and names](12-structure.md#decisions), U13, keeps how the targets are built and how criterion's filter chooses a case; this document decides what the cases are.

## Today

- **Inventory.** 28 cases in 9 groups: `num` (7), `rng` (4) and `vec3` (5) in `math`; `collision` (2) in `capabilities`; `state_hash` (2) in `sim`; `chain_head` (2) in `protocol`; `tick_3v3` (3) in `runner`; `rollback` (2) and `match_1v1` (1) in `net`.
- **Shapes.** Group names mix the path measured (`state_hash`, `collision`), the scenario (`match_1v1`) and both (`tick_3v3`). Case names mix a workload (`crowded`), a statistic (`mean`) and a sentence (`worst_of_a_match_checkpointed`). `num`, `vec3` and `collision` state their throughput; the others do not, so their figures are per iteration of a size nobody reads. `num`, `rng`, `vec3` and `state_hash` have no doc comment that states what they measure.
- **Scale.** `collision` takes 1,000 bodies, the RTS genre's count; `state_hash` takes 300 entities of made-up components; `num` and `vec3` take 4,096 inputs and `rng` one.
- **Ends.** `rollback/*` times `InProcessMatch::step`, a client frame and the server's frames together, under a name that says client. No case times a server frame.
- **Gaps.** Profiled on a Ryzen 7 6800U over `tick_3v3/mean` (157 µs a tick), the tick's time goes to: `orders::think` 42 %, of which 32 points are Rhai, the AI's `on_think` calls; `vision::see` 15 %, 12 points of it the sight spans and their bitmap writes; `navigation::steer` 3.3 %; `combat::attack_events` 3.3 %; `navigation::collide` 2.7 %; `actions::hold_passives` 2.4 %; `navigation::plan_routes` 1.5 %; the rest under 1 % each. No case measures a stage of the tick, the AI, the script host or the fog, so the two paths that take 57 % of a tick have no bench, and a change to them shows only in `tick_3v3`, mixed with everything else.

## Decisions

- **B1. Three tiers, each with one kind of fixture.**
  - An **end** case runs the reference packages as a match plays them, and times one end: the server's tick or frame, or the client's frame. The 3v3 for the server's tick and its stages; the lane 1v1 for the frames of both ends, through `InProcessMatch`.
  - A **kernel** case runs one capability's work for a tick on a made scene of 1,000 units, the count design 04 sets for an RTS battle ([Genres](04-capabilities/genres.md)): crowded into 40 m square and spread over 120 m square when its cost grows with how close the units stand, as `collision` is now. One scene builder serves every kernel of `capabilities`.
  - A **primitive** case runs one operation over 4,096 inputs drawn from `split_mix`, or over one input when the operation costs more than a microsecond, as a signature does.
- **B2. One rule for a case's id.** The group is the path measured, a noun: `server_tick`, `server_stage`, `server_frame`, `client_frame`, `collision`, `fog`, `script`, `state_hash`, `snapshot`, `chain_head`, `num`, `rng`, `vec3`. The case is the workload: an end case's is its statistic and scenario, `mean_3v3`, `worst_3v3`, `walk`, `worst_1v1`; a kernel case's is its scene, `crowded` or `spread`, or its variant when density does not change its cost; a primitive's is its operation. An end has a `mean` and a `worst` case where a match has both, since the worst tick is the budget ([Tick rate](04-capabilities/00-overview.md#tick-rate)).
- **B3. Every case states its unit.** A case whose iteration runs more than one of its cost unit, units, calls or inputs, states `Throughput::Elements` of that count, so criterion reports each one's cost. Each bench function's doc comment names the path, the scale and the cost unit.
- **B4. An end case times one end.** A client case times only `client_frame`, a server case only `server_frame` or the runner's tick, by `iter_custom`, never a step that runs both.
- **B5. A stage case times one stage of the real tick.** `sim`'s `StageClock::install`, which its `internals` give, adds to a built `SimUpdate` schedule a probe at each of the ten edges around the nine stages, each ordered between its two stages, which writes the time into the `StageClock` resource, outside the sim's state: it touches no component and no hashed resource, so the match and its hashes stay the same. A stage's time in a tick is between its two probes. Work that a capability runs between two stages belongs to the stage before it: `sim` gives each stage a closing set, `SimEdge::After(stage)`, ordered after the stage and before the next, and `SimEdge::Start` before the first stage, and a capability puts such work into one, as `stats` puts its refresh of the stats a stage changed. Each probe runs after the closing set of the stage before it and before `end_tick`, and the first after `SimEdge::Start` and `start_tick`, so a stage's time holds its closing set, and `SimEdge::Start`, `start_tick` and `end_tick` fall outside every stage, beside the session log's part of a tick. A system of the tick in no stage and no edge set fails a test of the proving match's schedule, so no work falls between the probes in an order Bevy picks. Before the closing sets, `stats`' refresh passes and `navigation`'s `keep_in_bounds` had no order against the probes, and the record's nine means added up to 159.3 µs, more than the 155.8 µs tick; with them they add up to 164.5 µs, under the 171.3 µs tick of the same run. The proving match plays to both goldens with the probes in its schedule, beside the reverse query order. The ten probes of a tick cost about 0.2 µs, 0.1 % of a 3v3 tick.
- **B6. A kernel for each path that passes 10 % of a tick.** Every stage has a case in `server_stage`; a path whose stage passes 10 % of the mean or the worst 3v3 tick also gets a kernel, which isolates it from the match. The [record](#record) finds five: `think` and `vision`, which have their kernels, and `resolve`, `inputs` and `move`, which wait for theirs. A capability's Cost section names the case that measures it and the figure, with the machine, as navigation's names `collision` now; Stage 11's "each capability's worst tick measured against its stated cost" adds one kernel for each other Cost section then.
- **B7. Profiling.** A profile of a case comes from a build with frame pointers in its own target directory, so the usual build is not rebuilt: `RUSTFLAGS="-C force-frame-pointers=yes" cargo bench -p <crate> --features bench --no-run --target-dir target/frame-pointers`, then `perf record --call-graph fp` on the bench binary with `--bench <id> --profile-time 10`. DWARF unwinding lost the stacks of the optimized build, and AMD processors have no LBR. Frame pointers cost no measurable time in this workload: `server_tick/mean_3v3` measured 156.8 µs without and 156.3 µs with them, within the noise. The usual build stays as it is: a profile is made on request, and a change of `RUSTFLAGS` builds every crate again.

## The suite

| Tier | Crate | Case | Now | Measures |
| --- | --- | --- | --- | --- |
| End | `runner` | `server_tick/mean_3v3` | `tick_3v3/mean` | The mean tick of the 3v3, 6,000 ticks |
| End | `runner` | `server_tick/worst_3v3` | `tick_3v3/worst_of_a_match` | The worst tick of a 3v3 match |
| End | `runner` | `server_tick/worst_3v3_checkpointed` | `tick_3v3/worst_of_a_match_checkpointed` | The same, with the main thread's part of a checkpoint every 100 ticks |
| End | `runner` | `server_stage/mean_3v3_<stage>`, 9 | new | Each stage's mean time a tick in the 3v3 |
| End | `runner` | `server_stage/worst_3v3_<stage>`, 9 | new | Each stage's worst time in a 3v3 match |
| End | `net` | `server_frame/walk` | new | The server's frame while the solo avatar walks |
| End | `net` | `server_frame/worst_1v1` | new | The server's worst frame in the lane 1v1, with its journal |
| End | `net` | `client_frame/walk` | `rollback/frame_without_rollback` | The client's frame while its avatar walks, rolling back only on a misprediction |
| End | `net` | `client_frame/walk_rollback` | `rollback/frame_with_rollback` | The same, rolling back on every confirmed update |
| End | `net` | `client_frame/worst_1v1` | `match_1v1/worst_client_frame` | Either client's worst frame in the lane 1v1, with its receipts |
| Kernel | `capabilities` | `collision/crowded`, `collision/spread` | the same | The Collide stage for 1,000 bodies |
| Kernel | `capabilities` | `fog/sight` | new | The Vision stage's grid fog for 1,000 units with sight: their spans, the bitmaps, and the teams that see each. One case: it measured 446 µs crowded and 452 µs spread, as each sight reveals as many cells however close the units stand |
| Kernel | `script` | `script/call` | new | 1,000 calls of an empty hook through `ScriptHost::call`: what each AI think pays before its script runs |
| Kernel | `script` | `script/native` | new | 1,000 calls of a hook that makes one call to a function the host registered, as each `ctx` query is |
| Kernel | `sim` | `state_hash/all`, `state_hash/by_type` | `state_hash/moba_300`, `moba_300_by_type` | The state hash of 1,000 units |
| Kernel | `sim` | `snapshot/all` | new | The snapshot of the same 1,000 units, as a checkpoint writes it |
| Primitive | `protocol` | `chain_head/sign`, `chain_head/check` | the same | One packet's signature, made and checked |
| Primitive | `math` | `num/*`, `vec3/*`, `rng/*` | the same | Each operation over 4,096 inputs |

The nine stages are `inputs`, `think`, `act`, `move`, `collide`, `hit`, `resolve`, `mode` and `vision`, `SimSet::ALL`. `server_stage` and `server_tick` measure the same matches, so their means agree within the session log's part of a tick, which runs outside the stages.

## Cost in CI

CI runs each case once ([U13](12-structure.md#decisions)). Each `worst` case of an end plays a whole match. Run 37518789388 measured the step: after a build of 4 to 12 s, its cases ran 30.2 s on Ubuntu, 31.8 s on macOS and 34.2 s on Windows, of which about 20 s is the eleven cases that each play a whole 3v3 in the dev profile. That passes the 30 s a suite may take, where this design estimated 17 s. A bench step is no test suite, as a case runs as it measures, so it has its own limit, and every case stays. The limit is 60 s: the mean cases play whole matches too, which adds about 17 s to the step, and a stage's worst is what shows a change that moves the worst tick, which is the budget; the platforms run the step in parallel.

## Record

The whole suite on one core of a Ryzen 7 6800U, criterion's median of each case, with `cargo bench --workspace --features bench --bench '*'`. The `server_tick/mean_3v3`, `server_tick/worst_3v3` and `server_stage` rows come from a second run pinned to one core, after the closing sets of [B5](#decisions), with `--bench 'server_tick/(mean|worst)_3v3$|server_stage/'`. The harness plays the 3v3 at 20 Hz.

| Case | Median | Case | Median |
| --- | --- | --- | --- |
| `server_tick/mean_3v3` | 171.3 µs | `server_stage/mean_3v3_inputs` | 7.2 µs |
| `server_tick/worst_3v3` | 3.22 ms | `server_stage/mean_3v3_think` | 73.8 µs |
| `server_tick/worst_3v3_checkpointed` | 3.15 ms | `server_stage/mean_3v3_act` | 4.5 µs |
| `server_frame/walk` | 52.1 µs | `server_stage/mean_3v3_move` | 10.8 µs |
| `server_frame/worst_1v1` | 303.3 µs | `server_stage/mean_3v3_collide` | 6.1 µs |
| `client_frame/walk` | 45.8 µs | `server_stage/mean_3v3_hit` | 13.0 µs |
| `client_frame/walk_rollback` | 88.8 µs | `server_stage/mean_3v3_resolve` | 22.0 µs |
| `client_frame/worst_1v1` | 301.1 µs | `server_stage/mean_3v3_mode` | 3.3 µs |
| `collision/crowded` | 306.3 µs | `server_stage/mean_3v3_vision` | 23.9 µs |
| `collision/spread` | 79.1 µs | `server_stage/worst_3v3_inputs` | 1.66 ms |
| `fog/sight` | 443.9 µs | `server_stage/worst_3v3_think` | 178.7 µs |
| `script/call` | 59.5 µs | `server_stage/worst_3v3_act` | 17.3 µs |
| `script/native` | 392.4 µs | `server_stage/worst_3v3_move` | 901.5 µs |
| `state_hash/all` | 263.3 µs | `server_stage/worst_3v3_collide` | 23.5 µs |
| `state_hash/by_type` | 264.9 µs | `server_stage/worst_3v3_hit` | 55.0 µs |
| `snapshot/all` | 202.0 µs | `server_stage/worst_3v3_resolve` | 120.8 µs |
| `chain_head/sign` | 17.5 µs | `server_stage/worst_3v3_mode` | 180.0 µs |
| `chain_head/check` | 25.6 µs | `server_stage/worst_3v3_vision` | 61.0 µs |

`num`, `vec3` and `rng` run 4,096 inputs a case: `num/mul` 7.5 µs, `num/div` 15.9 µs, `num/sqrt` 35.2 µs, `num/sin_cos` 75.4 µs, `num/sin_cos_huge` 75.7 µs, `num/atan2` 85.7 µs, `num/atan2_near_axis` 85.2 µs; `vec3/dot` 13.8 µs, `vec3/distance` 51.0 µs, `vec3/within` 11.0 µs, `vec3/normalized` 132.7 µs, `vec3/rotated_y` 19.0 µs; `rng/open` 175.4 µs, `rng/next_u64` 30.4 µs, `rng/below` 32.0 µs, `rng/chance` 31.7 µs.

**The mean cases.** Each sample of a mean case runs a different stretch of a match, from its quiet start to its fights: the ten samples of `server_tick/mean_3v3` ran 75.8 µs to 245.5 µs a tick. So its median is not the mean of a match, and it moved from 155.8 µs to 171.3 µs between two runs of the same tick. Two mean cases agree only to about 10 %, and the nine stage means fall under the tick's in this run, not in every run. So a mean case's iteration becomes one whole match, which states 6,000 ticks as its throughput, as the worst cases play whole matches already: criterion then reports a tick's mean over whole matches.

**The stages past 10 % of a tick** ([B6](#decisions)). Of the mean tick, 171.3 µs: Think, 73.8 µs, 43 %; Vision, 23.9 µs, 14 %; Resolve, 22.0 µs, 13 %. Of the worst tick, 3.22 ms, as a budget: the worst Inputs stage, 1.66 ms, 51 %, and the worst Move stage, 0.90 ms, 28 %, each in a tick of its own. Think has its kernel in `script`, Vision in `fog`; Resolve, Inputs and Move have none yet. The worst tick is 19 times the mean, and two stages that cost little on average make most of it.

## Hot stages

Where the three stages with no kernel spend their time, from three matches of the reference 3v3 at 20 Hz on one core, with the probes of [B5](#decisions): each match gives the same ticks. Each peak is profiled alone: `perf record -k CLOCK_MONOTONIC --call-graph fp` over the matches, built as [B7](#decisions) says, and `perf report --time` over the spans of the peak ticks, which a scratch program, not kept, printed from the probes on the same clock. Each stage below names its path and the kernel it takes; the kernels were reviewed, and the plan builds the script view's first.

- **Inputs: the pathing grid at the first tick.** Its worst is tick 0 alone, 1.99 ms; no later tick passes 0.10 ms. In tick 0 the map's static bodies enter, and `navigation`'s `track_static_bodies` updates the pathing grid of half-meter cells over the 96 m by 136 m map: `PathingGrid::update` marks their cells, and `Regions::label` labels again each chunk they touch, nearly all of the stage. `server_stage/worst_3v3_inputs` already measures that tick and nothing else, so a kernel of it would measure it twice. What a match pays again is one static body that enters later, a tower that dies or a building an RTS places: the update labels only the chunks it touches. Proposed kernel, in `capabilities`: `pathing_grid/one`, one static body more on a grid of half-meter cells over the spread scene that holds its other static bodies, one in four of its 1,000. No kernel for the first tick: it costs 2 ms of a 50 ms tick once a match.
- **Move: the routes of a wave.** Its worst ticks are the waves' spawns, ticks 2400, 3000 and each 600 after, 0.97 ms to 1.04 ms each, and the tick after each, about 0.6 ms: each wave's 24 creeps, six for each team in each of the two lanes, plan their lane routes. `navigation`'s `plan_routes` takes 71 % of those ticks, in `RoutePlanner::plan`: `Clearance::walled`, 52 %, the exact test of each line of a route against the cells the walls block, and `BodyIndex::blocks`, 20 %, its test against the static bodies. The planner stops a tick's work at its limit, as many units of work as the grid has cells, and the rest waits for the next tick: that limit is what makes the peak and the tick after it. So the figure that bounds the stage is the time of one unit of work, which a kernel gives when it states its work as its throughput. A scene of bodies alone has no walls, and `Clearance::walled` then returns at once, so it would leave out half the cost: the kernel's scene holds walls. Proposed kernel, in `capabilities`: `route_planner/walled`, 100 routes from one side of the spread scene to the other, around its static bodies and across walls that split it into lanes and a jungle, as the 3v3's map does; its throughput is the planner's units of work, and navigation's Cost section states the time of the tick's limit. Each of a group's six creeps plans the same route from nearly the same cell; that is navigation's to decide, not the kernel's.
- **Resolve: the damage hook and the script view.** Its cost is the mean, not a peak: its worst ticks, 0.13 ms to 0.18 ms, fall in the fights. `combat`'s `DamagePass::run` takes 7.7 % of the whole run, 60 % of the stage: half of it the mode's `calc_damage` hook, one call for each damage, and most of the rest `View::read`. That read is a copy of the whole match for the scripts: each `ScriptBatch::run`, of the eight in the capabilities, reads every unit again and fills its row with the column of each of the seven sources, so the scripts of a stage read the match as the stage began. `View::read` takes 14.2 % of the whole run, in Think, 7.9 %, the damage pass, 2.9 %, and the rest: a path past 10 % of the mean tick on its own, which no kernel measures. Between two batches of one tick, few units change, and every row is filled again. Proposed kernel, in `capabilities`: `script_view/read`, the read of the spread scene's 1,000 units with the columns of every capability the reference 3v3 declares, its throughput the units; and `script_view/reread`, the next read after one unit in a hundred changed, as a second batch of a tick finds them. Now the two cost the same; a view that reads only what changed makes the second cheap, and the pair shows it. The hook's own call is `script/call`'s and `script/native`'s already; the damage pass's own work, 4 % of it, takes no kernel.

## Plan

The steps are in [PLAN.md](../../PLAN.md#benches).
