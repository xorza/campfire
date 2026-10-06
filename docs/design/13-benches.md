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
- **B5. A stage case times one stage of the real tick.** `sim`'s `StageClock::install`, which its `internals` give, adds to a built `SimUpdate` schedule a probe at each of the ten edges around the nine stages, each ordered between its two stages, which writes the time into the `StageClock` resource, outside the sim's state: it touches no component and no hashed resource, so the match and its hashes stay the same. A stage's time in a tick is between its two probes. The proving match plays to both goldens with the probes in its schedule, beside the reverse query order. The ten probes of a tick cost about 0.2 µs, 0.1 % of a 3v3 tick.
- **B6. A kernel for each path that passes 10 % of a tick.** Every stage has a case in `server_stage`; a path whose stage passes 10 % of the mean or the worst 3v3 tick also gets a kernel, which isolates it from the match. Now that is `think` and `vision`. A capability's Cost section names the case that measures it and the figure, with the machine, as navigation's names `collision` now; Stage 11's "each capability's worst tick measured against its stated cost" adds one kernel for each other Cost section then.
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

CI runs each case once ([U13](12-structure.md#decisions)). Each `worst` case of an end plays a whole match. Run 37518789388 measured the step: after a build of 4 to 12 s, its cases ran 30.2 s on Ubuntu, 31.8 s on macOS and 34.2 s on Windows, of which about 20 s is the eleven cases that each play a whole 3v3 in the dev profile. That passes the 30 s a suite may take, where this design estimated 17 s; [PLAN_QUESTIONS.md](../../PLAN_QUESTIONS.md) holds the choice.

## Record

The whole suite on one core of a Ryzen 7 6800U, criterion's median of each case, with `cargo bench --workspace --features bench --bench '*'`:

| Case | Median | Case | Median |
| --- | --- | --- | --- |
| `server_tick/mean_3v3` | 155.8 µs | `server_stage/mean_3v3_inputs` | 6.9 µs |
| `server_tick/worst_3v3` | 3.13 ms | `server_stage/mean_3v3_think` | 70.7 µs |
| `server_tick/worst_3v3_checkpointed` | 3.15 ms | `server_stage/mean_3v3_act` | 4.4 µs |
| `server_frame/walk` | 52.1 µs | `server_stage/mean_3v3_move` | 11.5 µs |
| `server_frame/worst_1v1` | 303.3 µs | `server_stage/mean_3v3_collide` | 5.9 µs |
| `client_frame/walk` | 45.8 µs | `server_stage/mean_3v3_hit` | 11.2 µs |
| `client_frame/walk_rollback` | 88.8 µs | `server_stage/mean_3v3_resolve` | 20.0 µs |
| `client_frame/worst_1v1` | 301.1 µs | `server_stage/mean_3v3_mode` | 2.4 µs |
| `collision/crowded` | 306.3 µs | `server_stage/mean_3v3_vision` | 26.3 µs |
| `collision/spread` | 79.1 µs | `server_stage/worst_3v3_inputs` | 1.66 ms |
| `fog/sight` | 443.9 µs | `server_stage/worst_3v3_think` | 233.6 µs |
| `script/call` | 59.5 µs | `server_stage/worst_3v3_act` | 20.0 µs |
| `script/native` | 392.4 µs | `server_stage/worst_3v3_move` | 903.9 µs |
| `state_hash/all` | 263.3 µs | `server_stage/worst_3v3_collide` | 25.9 µs |
| `state_hash/by_type` | 264.9 µs | `server_stage/worst_3v3_hit` | 62.5 µs |
| `snapshot/all` | 202.0 µs | `server_stage/worst_3v3_resolve` | 125.4 µs |
| `chain_head/sign` | 17.5 µs | `server_stage/worst_3v3_mode` | 167.3 µs |
| `chain_head/check` | 25.6 µs | `server_stage/worst_3v3_vision` | 74.6 µs |

`num`, `vec3` and `rng` run 4,096 inputs a case: `num/mul` 7.5 µs, `num/div` 15.9 µs, `num/sqrt` 35.2 µs, `num/sin_cos` 75.4 µs, `num/sin_cos_huge` 75.7 µs, `num/atan2` 85.7 µs, `num/atan2_near_axis` 85.2 µs; `vec3/dot` 13.8 µs, `vec3/distance` 51.0 µs, `vec3/within` 11.0 µs, `vec3/normalized` 132.7 µs, `vec3/rotated_y` 19.0 µs; `rng/open` 175.4 µs, `rng/next_u64` 30.4 µs, `rng/below` 32.0 µs, `rng/chance` 31.7 µs.

**The stages past 10 % of a tick** ([B6](#decisions)). Of the mean tick, 155.8 µs: Think, 70.7 µs, 45 %; Vision, 26.3 µs, 17 %; Resolve, 20.0 µs, 13 %. Of the worst tick, 3.13 ms: Inputs, 1.66 ms, 53 %, and Move, 0.90 ms, 29 %. Think has its kernel in `script`, Vision in `fog`; Resolve, Inputs and Move have none yet. The worst tick is 20 times the mean, and two stages that cost little on average make most of it.

## Plan

The steps are in [PLAN.md](../../PLAN.md#benches).
