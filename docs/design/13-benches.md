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
- **B2. One rule for a case's id.** The group is the path measured, a noun: `server_tick`, `server_stage`, `server_frame`, `client_frame`, `collision`, `fog`, `script`, `state_hash`, `chain_head`, `num`, `rng`, `vec3`. The case is the workload: an end case's is its statistic and scenario, `mean_3v3`, `worst_3v3`, `walk`, `worst_1v1`; a kernel case's is its scene, `crowded` or `spread`, or its variant when density does not change its cost; a primitive's is its operation. An end has a `mean` and a `worst` case where a match has both, since the worst tick is the budget ([Tick rate](04-capabilities/00-overview.md#tick-rate)).
- **B3. Every case states its unit.** A case whose iteration runs more than one of its cost unit, units, calls or inputs, states `Throughput::Elements` of that count, so criterion reports each one's cost. Each bench function's doc comment names the path, the scale and the cost unit.
- **B4. An end case times one end.** A client case times only `client_frame`, a server case only `server_frame` or the runner's tick, by `iter_custom`, never a step that runs both.
- **B5. A stage case times one stage of the real tick.** `sim`'s `bench`-gated `SimUpdate::time_stages` adds to a built `SimUpdate` schedule a probe at each of the ten edges around the nine stages, each ordered between its two stages, which writes the time into a `StageClock` resource outside the sim's state: it touches no component and no hashed resource, so the match and its hashes stay the same. A stage's time in a tick is between its two probes. The ten probes of a tick cost about 0.2 µs, 0.1 % of a 3v3 tick.
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
| End | `net` | `client_frame/worst_1v1` | `match_1v1/worst_client_frame` | Either client's worst frame in the lane 1v1 |
| Kernel | `capabilities` | `collision/crowded`, `collision/spread` | the same | The Collide stage for 1,000 bodies |
| Kernel | `capabilities` | `fog/crowded`, `fog/spread` | new | The Vision stage's grid fog for 1,000 units with sight: their spans, the bitmaps, and each unit's `SeenBy` |
| Kernel | `script` | `script/call` | new | 1,000 calls of an empty hook through `ScriptHost::call`: what each AI think pays before its script runs |
| Kernel | `script` | `script/native` | new | 1,000 calls of a hook that makes one call to a function the host registered, as each `ctx` query is |
| Kernel | `sim` | `state_hash/all`, `state_hash/by_type` | `state_hash/moba_300`, `moba_300_by_type` | The state hash of 1,000 units |
| Primitive | `protocol` | `chain_head/sign`, `chain_head/check` | the same | One packet's signature, made and checked |
| Primitive | `math` | `num/*`, `vec3/*`, `rng/*` | the same | Each operation over 4,096 inputs |

The nine stages are `inputs`, `think`, `act`, `move`, `collide`, `hit`, `resolve`, `mode` and `vision`, `SimSet::ALL`. `server_stage` and `server_tick` measure the same matches, so their means agree within the session log's part of a tick, which runs outside the stages.

## Cost in CI

CI runs each case once ([U13](12-structure.md#decisions)). Each `worst` case of an end plays a whole match: the 3v3's take about 1.5 s each in the dev profile, so the nine `server_stage/worst_3v3_*` add about 14 s to each platform's step, now 3.4 s, and `server_frame/worst_1v1` less than 0.3 s, what `net`'s three cases take now. That stays under the 30 s a suite may take; a shorter match would measure the pick and the first wave only.

## Plan

The steps are in [PLAN.md](../../PLAN.md#benches).
