# Perf journal

The journal of the optimization session on branch `perf-change-driven-passes`, and of the
measurements after it, to 2026-10-24: facts only; [Benches](../design/02-engine-core.md#benches)
decides from them. Host: `local`, an AMD Ryzen 7 6800U (Zen 3+, 8 cores, SMT), governor `performance`. Runs are pinned with `taskset -c 12` on the bench binary; base and candidate interleave.

## Method on this host

- **The clock moves.** The laptop runs at 85 °C under its power limit, and the boost cannot be turned off without root. Each run is scaled by its process's mean clock, `perf stat -e cycles,task-clock`, to megacycles per iteration (`../campfire-perf/tools/ab.py`).
- **Between processes, not within.** Criterion's interval within a run is about ±0.2 %; the spread between runs of one binary is far larger. A verdict reads the median of the pairs, base and candidate run back to back.
- **Instructions as evidence of work removed.** Two runs of one binary differ by 0.0006 % in `instructions:u`. With `--warm-up-time 1e-9 --measurement-time 1e-9 --sample-size 10`, criterion runs a fixed 55 iterations, so `perf stat -e instructions:u` over the process counts a fixed workload.

## Noise floors

A/A of two builds of one tree, four rounds, clock-scaled:

| Host | Case | Spread of runs | Pairs | Medians |
| --- | --- | --- | --- | --- |
| local | `server_tick/mean_3v3` | 1.4 % | −1.49 to +1.38 % | +0.08 % |
| local | `battle/crowded_act` | 40 to 54 % | −25 to +12 % | −7.6 %: unusable as it is |
| local | `battle/crowded_resolve` | 13 to 15 % | −1.12 to +1.14 % | +0.27 % |

## Profiles at the start

Self time, `perf record` at 2 kHz on core 12, frame pointers. `mean_3v3`, inclusive: `Thinking::think` 45.7 %, of which the script calls 45.2 %; `Walking::steer` 4.0 %; the stat refresh 3.4 % (2.6 % self, its `Or<(Changed<…>, …)>` filter); `HeldPass::run` 3.3 % (0.7 % self, `UnitsNear::current` 2.0 %); `Walking::collide` 3.2 %; `Holds::want_passives` 1.0 %; `Refresh::regenerate` 0.8 %; `Holds::hold_charges` 0.47 %; `Toggles::run_toggles` 0.46 %; `GrantPass::run` 0.4 %; `BodyIndex::update` 0.73 % self, the first tick's take of every static body. The battle tick: the scripts lead, `Targets::living` 2.3 %, `Metric::reaches` 1.8 %, `Walking::collide` 1.9 %, `want_passives` 0.6 %.

## Current numbers

The tip, `f5989a97`, against the master it merged, `91d5f2ba`, four rounds, clock-scaled megacycles per iteration on `local`; the seeds and `push_out` cases, which master lacks, from their commit.

| Case | Master | Tip | Change |
| --- | --- | --- | --- |
| `server_stage/mean_3v3_inputs` | 156.94 | 138.39 | −11.8 % |
| `battle/crowded_inputs` | 24.027 | 21.412 | −10.9 % |
| `gather/two_fields` | 0.1476 | 0.1419 | −3.9 % |
| `collision/boxes` | 3.5920 | 3.5547 | −1.0 % |
| `body_box/reach_box` | 0.3173 | 0.3134 | −1.2 % |
| `battle/crowded_move` | 33.636 | 33.278 | −1.1 %, one pair of four the other way |
| `server_stage/mean_3v3_move` | 234.69 | 237.37 | +1.1 %, at its floor |
| `server_tick/mean_3v3` | 3386.3 | 3390.5 | +0.1 %, inside its floor |
| `server_tick/worst_3v3` | 2.2839 | 2.3014 | +0.8 %, pairs on both sides |
| `collision/crowded`, `body_box/reach`, `body_box/overlap` | | | −0.2 %, −0.3 %, +0.4 %: inside their floors |
| `server_seeds/segment_starts` | 167.01 | 0.0059 | 36.0 ms → 1.26 µs |
| `body_box/push_out` | 2.1746 | 1.4531 | −33.2 % |

## Ideas

| Idea | Evidence | Numbers | Verdict |
| --- | --- | --- | --- |
| Seeds: derive each segment's seed once, a table in `ServerSeeds` | `ServerSeeds::seed` hashes back from the last segment each call; the lobby's chain has 1,024 segments, and a segment's start in the server's tick takes up to 1,023 BLAKE3 hashes; `begin_checkpoint` derives one more only to test it is known | `server_seeds/segment_starts` 36.0 ms → 1.26 µs; `derive` 5 ns → 78.6 µs, once a session, which the old build also spent on segment 0's seed | kept |
| `BodyBox::push_out`: one `Edges` and one probe | `push_out` makes the probe three times and `Edges` twice for a point that overlaps: its own test, `Edges::against`'s, `nearest_to`'s | `body_box/push_out` −33.2 %, `reach_box` −1.3 %, `reach` −0.5 %, `collision/boxes` −1.0 %; `collision/crowded` +0.7 % and `body_box/overlap` +3.3 %, each with its machine code the same at another offset of its cache line | kept |
| `probe_against` out of line | the first cut split `Edges::against`, and LLVM inlined the wrapper and kept the probe's loop apart, so the probe went through memory | `body_box/reach` +4.5 %, `reach_box` +3.3 % | rejected: `#[inline(always)]` on `probe_against` |
| `DueRun` in `sim`; `hold_charges` and `run_toggles` visit what changed and what is due | each walked every unit's slots every tick, about 20 % of the Inputs stage (5.8 µs a 3v3 tick) | `server_stage/mean_3v3_inputs` −6.7 %, `battle/crowded_inputs` −6.9 %; `server_tick/worst_3v3` −1.3 %, `mean_3v3` +0.6 % within its floor, with 0.35 % fewer instructions over 55 matches | kept |
| `DueRun` for `Thinking::think` | its scan of the thinkers is about 0.3 µs of the 60 µs Think stage | below 1 % of every case that holds it | not tried: no case can show it |
| `route_units` visits a walker whose destination or route changed, or that lived again; every walker on a static change | with no static change, an unchanged walker leaves the pass as it found it, after a walk of its waypoints | `battle/crowded_move` −2.85 %, `mean_3v3_move` −0.2 % | kept |
| `keep_in_bounds` over changed positions | the pass is 0.12 % of the 3v3 tick | reading `MoveStep`'s ticks makes it unordered against `Refresh::run`, which writes `MoveStep` | rejected: an order against the stats refresh for a gain no case can show |
| Gather: a worker's search takes each node's parts from its iteration, and reads the match's relations once | `OnMatch<Relations>::get` is 10.5 % of `gather/two_fields`; `free_node_near` looked each node up again by id and read the relations for each | first cut, relations once a step: `gather/two_fields` +4.5 %, the same work at other places, as the bench calls `get` once a worker step, not in the search | rejected |
| Gather: the relations read once for each read of the gather view, passed to each step | the 200 workers' steps each read them | `gather/two_fields` −3.9 %; with routing, `server_tick/worst_3v3` −0.17 %, `mean_3v3` +0.56 % and `mean_3v3_act` +1.2 %, each with mixed pairs, 0.04 % fewer instructions | kept |
| Gather: waiting workers by node, a sorted list each run, so a freed node finds its first without a scan of every worker | `GatherLoop::free` is 5.5 % of `gather/two_fields`, about five frees a tick, each a scan of 200 workers | `gather/two_fields` +3.75 %: the list's sort of about 180 waits each run costs more than the scans it saves | rejected; a list kept between runs would have to follow each outside change of `Gatherer`, a rollback's among them, and would keep the waits of nodes that are gone |
| `want_passives` over an `Or<(Changed<…>, …)>` filter and a list of waiting units | its samples are the change-tick compares, about 1.7 ns a unit a run; a filter reads the same ticks | no case can show what is left | not tried: at the floor of a scan in Bevy, which keeps no list of what changed |
| `HeldPass`: re-derive the player modifiers' holds on a change of owner, tags or `PlayerModifiers` | the player modifiers take 0.17 % of the 3v3 tick; the pass's cost is the shared unit grid, 2.0 %, which another reader would build | below 1 % of every case that holds it | not tried: no case can show it |
| `plan_routes`: a list of waiting asks, by `Changed<Route>` | its scan reads one field of each unit's route, the cost of reading a change tick | | not tried: no work to remove |
| `Flying::lose_teleported`: homing projectiles by target | an observer of a teleport, a rare event; `Flying::fly`'s 0.67 % is the flight itself | | not tried: cold |
| `NavigationColumn::room_for` near the box | a script's spawn of a box walks every row of the view; no tick runs it | | not tried: cold, no case |
| One restore target for each replay | `Replay::check_snapshot` builds a match for each checkpoint, as a restore needs a world without sim entities | | not tried: a reused target needs a reset of the world that the sim does not have, on the verifier's cold path |
| `Admission`: a tree of buckets by `AddressKey` and a heap of their times | `admit` kept the buckets in a `Vec` it walked twice a handshake, so a flood from many addresses cost each handshake every bucket; a `HashMap` is banned and a fixed hasher would let an attacker who picks the addresses collide them | `admission/flood` −85.0 %; `admission/rejoins` +21.0 % | rejected: the derived order of the enum key costs more than a walk of 64 buckets |
| `Admission`: a tree of buckets by the key as one `u128`, one search a handshake through its entry, and a heap of `(time, key)` | the first variant's searches compared the enum key field by field | `admission/flood` 6.39 ms → 0.187 ms, −97.0 %, 1.56 µs → 46 ns a handshake; `admission/rejoins` −60.7 %, 41 → 16 ns | kept |
| `Walking::keep_in_bounds`, `StaticTracking`, `Tracking::stale` | 0.12 % and 0.34 % of the 3v3 tick, each a scan of change ticks | | not tried: at the floor of a scan |
| Bisect of the batch of charges and toggles: the old `run_toggles` against the tip | the tip's profile gave `run_toggles` 0.52 % of the tick, against 0.46 % before | the old pass: `mean_3v3_inputs` +2.2 %, `crowded_inputs` +2.7 %, so the due run paid | kept the due run, then the variant below |
| `run_toggles` keeps the set of units with a toggle on, and finds new ones by the change ticks of `SlotTimes` alone | only a unit with a toggle on has one to keep, and a toggle turns on only as the unit's times change; the due run's scan read five change ticks a unit | against the due run: `mean_3v3_inputs` −5.9 %, `crowded_inputs` −2.9 % | kept |
| `EntityIndex`: the match's and the players' named ids in an array by their place past `StableId::MATCH`, the rest in the tree | every `OnMatch::get`, `PlayerRows::of_player_slot` and `UnitsWorld::relations` searched the tree; `Targets::relation` per candidate, `RowSource::fill` per row | host `xxorza` (i9-13980HX), criterion against a saved baseline, one run each: `gather/two_fields` 37.27 → 36.19 µs, −2.9 %; `server_tick/mean_3v3` 470.8 → 456.2 ms, −3.1 % | kept |
| `GatherLoop::run` takes its change filter once more at its end | its own `Gatherer` writes came after the filter's last run, so the next run freed nodes by a scan and a sort of every node whenever a worker stepped, against its doc | host `xxorza`, criterion against a saved baseline, one run: `gather/two_fields` 35.19 → 34.99 µs, −1.3 % | kept |
| The Zero Hour check: the install's 36 archives hashed on every core, each into one buffer of 1 MiB with one check of its stamp, and a package's directories made with no sync, once each | a profile of `--play supply` put 69 % of its samples in SHA-256, on one thread, a new zeroed `Vec` and an `fstat` per MiB; the import wrote 315 directories, each with a sync of its parent on btrfs over LUKS, for files written with none | host `xxorza`, the check's run alone, two runs each: 17.6 s; 16.2 s with the parallel hash, as the largest archive, 333 MB, sets the hash's floor; 11.8 s with the directories unsynced | kept |
| `BotView::nearest_where` on the unit grid in growing rings, as Zero Hour's `getClosestObjects` walks its cells | a profile of the Zero Hour check's `--play base`, 2 kHz, `checks` profile, 2026-10-24, kept import | the search below 0.07 % of 16,899 samples, under any symbol of the bots; the install's SHA-256 61.6 %, `Regions::nearest` 2.2 % | dropped: the scan does not show |
| `DueIndex` as a binary heap and a place for each entity index, in place of two B-trees | four interleaved pairs, 2026-10-24 | `gather/two_fields` −24.7 %, `acquire/idle` −12.4 %, `server_tick/mean_3v3` −1.45 % and `battle/crowded_resolve` −0.14 %, inside their floors | kept, in `0d234250b` |
| `rollback_scan.rs` scans only the pairs that gained an entry, with the anchor rule rewritten | `integration/rollback_scan/idle_check_400_units` against `idle_400_disabled`, 2026-10-24: a client predicting 400 still units and a mover, no misprediction | 50.7 µs against 42.7 µs a frame of both ends: the check 8.0 µs, about 20 ns a predicted unit, 40 µs for 2,000, 0.24 % of a 60 Hz frame; the Zero Hour check's plays run no client to profile | dropped: the scan does not show |

## Session of 2026-10-25

The second session, over every bench of the workspace, from `afddf65d8`. Hosts: `local`, an Intel Core i9-13980HX (8 P-cores with SMT, CPUs 0–15, and 16 E-cores, CPUs 16–31), governor `powersave` with EPP `performance`; `mac`, an Apple M2 (4 P-cores and 4 E-cores, 8 GB), macOS, arm64.

### Method

- **One bench target a build.** A build of every bench at once, two trees in parallel, ran the local host out of memory; each tree builds one target at a time, with `-j 16` locally and `-j 4` on the Mac.
- **The build on the E-cores, the bench on CPU 12.** The local build runs under `taskset -c 16-31`, the bench binary under `taskset -c 12`; even so, a build beside a bench moved single pairs by up to 450 % and medians by 10 %. A verdict reads only runs with every build paused (`SIGSTOP`).
- **Clock-scaled time.** Each run is scaled by its process's mean clock, `perf stat -e cpu_core/cycles/u,task-clock` (on this hybrid CPU the counter is `cpu_core/cycles/u`, as `cycles:u` names the E-cores' counter too), to megacycles an iteration.
- **No fixed workload by instructions.** `Scale` sets each group's sample count and time, which outweigh criterion's command line, so the iteration count of a run varies and `instructions:u` over a process measures no fixed work.
- **LBR for call graphs.** This host is Intel, so `perf record --call-graph lbr` unwinds the bench build as it is, with no frame-pointer build; its stacks hold 32 branches, so the inclusive shares of deep stacks are low bounds.

### Noise floors

A/A, base and candidate built from one commit, four interleaved pairs, clock-scaled:

| Host | Case | Pairs | Median |
| --- | --- | --- | --- |
| local, a build on the E-cores | `server_tick/mean_3v3` | −3.45 to +5.20 % | −0.27 % |
| local, a build on the E-cores | `battle/crowded_act` | −18.66 to +2.17 % | +0.25 % |
| local, a build on the E-cores | `battle/crowded_move` | −0.78 to +2.33 % | +2.09 % |
| local, a build on the E-cores | `battle/crowded_resolve` | −0.73 to +12.78 % | +0.54 % |
| local, a build on the E-cores | `battle/crowded_think` | −3.84 to +2.62 % | −0.85 % |

### Profile of the 3v3 tick

`integration/server_tick/mean_3v3` on `local`, `perf record -F 2000 --call-graph lbr`, 20 s, every sample of the run, the matches' setup among them. Self time by crate: Bevy's systems, into which most passes inline, 26.1 %; Rhai 25.1 %; `campfire_script` 10.7 %; vision 8.5 %; navigation 8.0 %; units 6.4 %; math 3.5 %; `malloc` and `free` 2.3 %. The leading symbols: `rhai::Engine::resolve_fn` 4.31 %, `Fog::sight` 3.37 %, `Bitmap::set` 2.87 %, `GridBuild::refresh` 2.11 %, `exec_native_fn_call` 2.08 %, `TargetIndex::index` 2.04 %, `Refresh::run` 1.94 %, `Broadphase::contacts` 1.62 %, `ViewReader::read_rows` 1.62 %, `get_component_and_ticks` 1.55 %.

The stages' means of a 3v3 match, one run, a build on the E-cores: think 222.0 ms, move 60.0, vision 55.3, hit 52.2, resolve 50.9, collide 36.7, inputs 19.0, act 16.6, mode 7.8; 520 ms in all, 86.7 µs a tick.

### Ideas

| Idea | Evidence | Numbers | Verdict |
| --- | --- | --- | --- |
| Spike: Rhai's function resolutions kept for each script from call to call, through a patched Rhai 1.26.1 whose `call_fn_with_caches` takes the caller's `Caches` | `Engine::call_fn_with_options` builds new `Caches` for each call, so the first uses of each function in a call search the AST's and the engine's modules; `resolve_fn` is 4.31 % of the 3v3 run's self time | `local`, quiet, four pairs: `server_stage/mean_3v3_think` −1.18 % (every pair below 0), `mean_3v3_hit` −0.06 %, `mean_3v3_resolve` −0.31 %, `battle/crowded_think` −0.37 %, `server_tick/mean_3v3` −0.20 % | rejected: inside the floor of the tick; most of `resolve_fn`'s time is the hash of each call's argument types and the probe of a cache that already holds the function, which every call pays |
| Spike: a counted fog, as Zero Hour's shroud counts its lookers: each cell keeps the count of each group's sights on it, and a seer that moved updates only the cells its sight gained or lost, row by row; a seer that stood still, kept its group, detection and brush, costs nothing | `Fog::sight` 3.37 % and `Bitmap::set` 2.87 % of the 3v3 run's self time: each Vision stage clears the bitmaps and reveals every sight again; the vision stage is 55 ms of a 520 ms match | `local`, quiet, four pairs, every pair one way: `battle/crowded_vision` −64.1 %, `server_stage/mean_3v3_vision` −32.8 %, `server_tick/mean_3v3` −3.83 %, `worst_3v3_vision` −6.6 %, `worst_3v3` −0.3 %; `fog/still` −80.8 %, `fog/sight` −10.4 %, the build beside it; a test of 200 random ticks of 40 seers over three groups, two brushes, detection and reveals finds every cell's bit equal to a fog built again each tick | spike kept apart, in `../campfire-perf/fog`; landed as the counted fog (04-capabilities/vision.md, A stage that costs what changed) |
| `BodyGrid::rebuild` keeps its index when given the bodies it holds, in the same order, compared as they come | the script view's `TargetIndex` rebuilds its grid after every read, though a read between two stages with no move changes no body; `TargetIndex::index` 2.04 % and `GridBuild::refresh` 2.11 % of the 3v3 run's self time | `local`, quiet, four pairs: `body_grid/rebuild_same` −90.3 %; `body_grid/rebuild`, bodies that moved, +0.78 %, every pair above 0, as the compare pushed each body | first loop rejected for the second below |
| `TargetIndex::each_reached` tests the filter before the distance | the filter is a few bit tests, the reach `Metric::order`'s exact sum of squares, 2.77 % of the battle's self time; both are pure, and the charge counts each body the grid gives before either test, so results and charges stay the same | with the rebuild's first loop: `battle/crowded_think` −5.63 %, `server_stage/mean_3v3_think` −3.56 %, `server_tick/mean_3v3` −1.51 %, `script_view/read` −1.20 %, `worst_3v3` −0.61 %, `mean_3v3_hit` −1.15 %; `battle/crowded_resolve` +1.29 %, every pair above 0 | measured again with the rebuild's second loop |
| The rebuild's second loop: the bodies compared until the first that differs, then the rest copied in one `extend` | the first loop pushed each body | `body_grid/rebuild` −0.86 %, `rebuild_same` −84.5 %; with the filter first: `battle/crowded_think` −3.49 %, `mean_3v3_think` −0.79 %, `mean_3v3` −0.56 %, `battle/crowded_resolve` +1.25 %, every pair above 0 | bisected below |
| Bisect: the filter first alone | | `battle/crowded_think` −3.89 %, every pair below 0; `mean_3v3_think` −0.58 %, `mean_3v3` −0.47 %; `battle/crowded_resolve` +0.75 %, one pair of four below 0 | the filter first makes the think stage's gain; the compare adds to the resolve stage's cost, as `UnitGrid` rebuilds only after a move, so its compare almost never keeps the index |
| The compare only in `TargetIndex`, as `BodyGrid::refresh`; `rebuild` builds as before, through `index_placed`, which both share | the unit grid's compare almost never keeps its index | `local`, quiet: `battle/crowded_think` −6.30 %, `mean_3v3_think` −3.34 %, `mean_3v3` −1.31 % (three pairs of four below 0), `battle/crowded_resolve` −0.42 %; `body_grid/rebuild_same` −85.3 %, `body_grid/rebuild` +1.88 %, every pair above 0, its source unchanged; `mac`, three pairs, the second loop in `rebuild`: `battle/crowded_think` −5.68 %, `mean_3v3_think` −2.31 %, `mean_3v3` −1.17 %, `battle/crowded_resolve` −0.43 % | `rebuild`'s cost bisected below |
| Spike, measured again on `local`: the target index split by team and tags | as above | `local`, quiet, four pairs, each the same way: `battle/crowded_think` −22.1 %, `mean_3v3_think` −6.76 %, `server_tick/mean_3v3` −4.00 %; `battle/crowded_resolve` −0.24 % | proposed |
| Spike, measured on `mac`: the counted fog | as above | three pairs, each the same way: `battle/crowded_vision` −60.0 %, `mean_3v3_vision` −34.5 %, `server_tick/mean_3v3` −3.32 % | proposed |
| `index_placed` inlined into `rebuild` and `refresh` | the third candidate's `rebuild` lost 1.88 % on `local` with its source unchanged; on `mac` it lost nothing (+0.06 %) | `local`, quiet: `body_grid/rebuild` −0.93 %, `rebuild_same` −84.3 % | kept, in the candidate |
| `ViewRows` searches a dense array of its rows' stable ids, taken again at the end of each read that filled the rows | a lookup by id binary-searches rows of about 180 bytes; `View::unit` 1.2 % of the battle's self time | with the candidate: `battle/crowded_think` −7.81 %, `mean_3v3_think` −3.93 %, `mean_3v3` −1.30 %, as without it; `script_view/reread` +4.70 %, `script_view/read` +0.84 %, as each read copies every id again | rejected: no gain on the tick, a loss on a read of a few changes |

#### The Zero Hour play's navigation

`integration/zero_hour_tick/mean_bases` on `local`, `perf record -F 2000 --call-graph lbr`, 30 s: `RoutePlanner::expand` 9.58 %, `RoutePlanner::passable` 5.59 %, `Regions::nearest` 5.06 %, `Grid::box_distance` 4.81 % (4.20 % of it from `Regions::nearest`'s scan, a call for each cell), `Broadphase::contacts` 4.67 %, `GridBuild::refresh` 4.33 %, `CrowdCosts::count` and its closures 8.70 %, `BodyBox::push_out` 3.10 %, `Refresh::run` 2.65 %, `PathingGrid::new` 2.16 % (the match's build, not a timed tick), `OpenCells::pop` 1.51 %.

| Idea | Evidence | Numbers | Verdict |
| --- | --- | --- | --- |
| A batch of three, with the candidate above: `Regions::nearest` takes each column's square once a region and each row's once a row, skips a row whose square passes the best distance, and pops its regions from a heap; `CrowdCosts::count` steps each run's column-major bit by the row count, where each cell took a division and a remainder; `RoutePlanner::expand` tests each straight cell once, where each diagonal tested its two straight cells again | the profile above; each change keeps the results, and `nearest` its work count, which the regions' test checks against the old scan, kept in the test | `local`, quiet, four pairs: `route_planner/stuck` −13.2 %, `priced` −10.1 %, `yield_place` −9.9 %, `walled` −9.2 %, `group_alone` −8.7 %, `group` −8.1 %, `line` +0.08 %, `nearest_open` −0.4 %; `crowd_costs/update_100_changes` −5.65 %, `update_1000_changes` −2.91 %, `update_10_changes` −1.50 %; three pairs: `zero_hour_tick/mean_attack` −16.7 %, `mean_bases` −11.0 %, `worst_bases` −5.8 %, `worst_attack` +135 %, every pair the same way; the Zero Hour check passes, so both plays run the same | the worst tick bisected below |
| `mac`, the batch with the heap | | three pairs: `zero_hour_tick/mean_attack` −15.8 %, `mean_bases` −8.76 %, `worst_attack` −1.20 %, `worst_bases` +3.79 % (pairs +3.53 to +3.95 %); `route_planner/stuck` −11.9 %, `yield_place` −9.11 %, `priced` −4.66 %, `walled` −4.36 %; `crowd_costs/update_100_changes` −2.70 %, `update_1000_changes` −2.62 %, `update_10_changes` +0.14 % | |
| Bisect: `Regions::nearest` sorts its regions again, as before; the scan by squares and the row skip stay | the heap's pops cost more than one sort when a scan visits many regions | `local`, three pairs: `zero_hour_tick/mean_attack` −17.7 %, `worst_attack` +3.03 %; an A/A of `base` against itself, three pairs: `worst_attack` −6.34 to +1.92 %, `worst_bases` −5.67 to +5.33 %, so `local`'s floor of a Zero Hour worst tick is about ±6 % | the heap rejected; the sort kept |
| `mac`, the batch with the sort | | three pairs, each the same way: `zero_hour_tick/mean_attack` −16.4 %, `mean_bases` −10.4 %, `worst_attack` −1.14 %, `worst_bases` −13.2 % | kept |
| The candidate on the capabilities' scenes | every scene of `campfire-capabilities`, for a loss on a path the batches did not aim at | `local`, three pairs: `navigation/worst_corridor` −17.1 %, `worst_crowd` −16.1 %, `mean_crowd` −3.50 %, `mean_corridor` −2.90 %, each pair the same way; `stats/worst_auras` −2.58 %; the other means within ±0.8 %; `navigation/worst_open` +8.71 % and `combat/worst_melee` +5.81 %, pairs on both sides; then six pairs: `combat/worst_melee` −3.45 %, every pair below 0, `navigation/worst_open` +1.25 %, against an A/A of −4.15 to +4.53 % | no loss |
| `CrowdCosts::update` counts a moved unit, of its team and standing on its layer, only on the cells each kind's footprint gained or lost, row by row, as the counted fog does; a unit of another team, standing or layer is taken away and added whole, as before | the candidate's profile of `zero_hour_tick/mean_attack`: `CrowdCosts::count` and its closures 15.8 %, as each moved unit took away and added its whole footprint for each walker kind | `local`, quiet, against `base`, with the whole candidate: `crowd_costs/update_1000_changes` −62.8 %, `update_100_changes` −49.2 %, `update_10_changes` −21.0 %; `zero_hour_tick/mean_attack` −30.9 %, `mean_bases` −17.3 %, `worst_bases` −8.83 %, `worst_attack` −2.10 %, every pair the same way; a test of 40 random updates of 30 units, moves, team and standing changes, finds the counts kept equal to a new count of the same stamps, and fails with the lost cells left out | kept |
| `mac`, the candidate up to the sort, on the capabilities' scenes and the 3v3 | | three pairs: `navigation/worst_corridor` −16.1 %, `worst_crowd` −16.1 %, `mean_crowd` −3.41 %, `mean_corridor` −3.04 %; `server_tick/worst_3v3` −15.8 %, `mean_3v3` −1.05 %, `mean_3v3_think` −1.57 %, `battle/crowded_think` −4.68 %, each pair the same way; `battle/crowded_resolve` +0.65 % (pairs +0.18 to +2.67 %, against an A/A of −1.37 to +0.75 %); every other scene within −1.32 to +0.50 % | no loss |
| `Regions::nearest` scans, in each row, only the run of columns whose squares leave room under the best distance, found by two binary searches either side of the goal's column, as the squares fall to it and rise after it | the candidate's profile: `Regions::nearest` 7.47 %, nearly all in its loop over a region's cells; the sort 0.3 % | `local`, against `base`, with the whole candidate: `zero_hour_tick/mean_attack` −35.0 %, `mean_bases` −18.2 %, `worst_bases` −8.38 %, `worst_attack` −1.52 %; `route_planner/stuck` −12.4 %, `yield_place` −9.58 %, `priced` −8.08 %, `group` −7.67 %, `walled` −7.32 %, each pair the same way; `route_planner/line` +0.93 %, its code unchanged, every pair above 0; the regions' test still finds each cell and work count equal to the old scan's | kept |

#### Not tried

| Idea | Evidence | Verdict |
| --- | --- | --- |
| Rhai's progress callback less often than each operation | `ScriptHost::new`'s progress closure is 0.96 % of the 3v3's self time; Rhai calls it at each operation, and a call's limit and its pool's budget count by it | not tried: Rhai offers no period, and a count by other steps changes where a script's budget ends |
| The view's rows' ids in a dense array, kept by the read's merge in place of a copy at its end | the copy cost `script_view/reread` 4.7 % | not tried: the lookup it would speed is no more than 1.2 % of the battle, and the merge's insertions and removals would each move the ids too |
| One spatial index of the units a tick, which the unit grid, the script view's targets, the broadphase and the crowd costs share | each builds its own from the same bodies: `GridBuild::refresh` 5.6 %, `Broadphase::contacts` 1.5 % and the crowd's recount 3.9 % of the candidate's attack play | not tried: each indexes another set by another key (layers, targets, walker kinds), a redesign past this session |
| `UnitGrid` and `Broadphase` sorted from the order of the tick before, which a run-adaptive sort takes in a pass | their sorts of `u128` keys are 0.5 to 1 % of a Zero Hour play | not tried: below what a play's floor of about ±6 % shows on a worst tick |
| The client bench `client` | the candidate changes no code the client's fog upload or picking runs | not run: its fat-LTO link swapped the Mac's 8 GB, and the local host's was stopped to free the build directory |
| Bisect of the final A/B's local losses: the navigation part alone (`Regions::nearest`, `CrowdCosts`, `RoutePlanner::expand`, `Grid::center_square`) | the final A/B on `local`: `body_index/near` +3.43 %, `acquire/idle` +2.03 %, `areas/mean_fields` +1.48 %, `abilities/mean_casts` +1.23 %, beyond A/A floors of ±0.15 %, ±0.75 %, ±0.3 % and ±0.25 %; on `mac`: −0.02 %, −0.44 %, +0.81 %, +0.01 % | the navigation part alone, `local`: `body_index/near` +3.91 %, `acquire/idle` +2.00 %, `areas/mean_fields` +1.55 %, `abilities/mean_casts` +1.25 % | kept: `body_index/near` runs no changed code, and the Mac shows none of the four but `areas/mean_fields`, so the navigation crate's code placement on x86 moves them; `route_planner/line`, also unchanged, loses on both hosts (+0.71 %, +1.74 %), as does `areas/mean_fields` |

### Current numbers

The session's kept changes, `078b2c269`, against their start, `afddf65d8`, 2026-10-25, three interleaved pairs: `local` in clock-scaled megacycles an iteration, quiet; `mac` in ns. Every pair the same way unless the change says so.

| Case | `local` start → end | Change | `mac` start → end | Change |
| --- | --- | --- | --- | --- |
| `zero_hour_tick/mean_attack` | 3263 → 2130 | −34.7 % | 680.2 → 457.2 ms | −32.8 % |
| `zero_hour_tick/mean_bases` | 9878 → 8071 | −18.3 % | 1.884 → 1.530 s | −18.8 % |
| `zero_hour_tick/worst_attack` | 20.59 → 20.25 | −1.66 % | 1.261 → 1.236 ms | −1.82 % |
| `zero_hour_tick/worst_bases` | 36.04 → 33.79 | −6.25 % | 5.478 → 4.751 ms | −13.1 % |
| `server_tick/mean_3v3` | 2709 → 2692 | −0.72 % | 521.4 → 515.5 ms | −1.24 % |
| `server_tick/worst_3v3` | 25.78 → 21.23 | −17.7 % | 6.056 → 5.109 ms | −15.6 % |
| `server_stage/mean_3v3_move` | 318.0 → 303.2 | −4.67 % | 65.78 → 62.84 ms | −4.55 % |
| `server_stage/mean_3v3_think` | 1172 → 1165 | −0.96 %, one pair −0.03 % | 226.7 → 223.3 ms | −1.49 % |
| `battle/crowded_move` | 39.61 → 30.23 | −23.7 % | 7.924 → 5.774 ms | −27.0 % |
| `battle/crowded_think` | 435.8 → 411.2 | −5.79 % | 87.39 → 82.57 ms | −5.57 % |
| `battle/crowded_resolve` | 155.8 → 155.4 | −0.27 % | 31.11 → 30.98 ms | −0.37 % |
| `server_frame/worst_walk_3v3` | 12.71 → 10.85 | −14.9 % | 3.385 → 2.875 ms | −14.7 % |
| `server_frame/mean_walk_3v3` | 910.4 → 905.2 | −0.57 % | 126.9 → 124.2 ms | −2.13 % |
| `navigation/worst_corridor` | 98.60 → 80.81 | −18.0 % | 23.39 → 19.57 ms | −16.3 % |
| `navigation/worst_crowd` | 56.53 → 47.20 | −16.7 % | 13.21 → 11.09 ms | −16.2 % |
| `navigation/mean_crowd` | 6631 → 6324 | −4.53 % | 1.484 → 1.419 s | −4.43 % |
| `navigation/mean_corridor` | 12320 → 11860 | −3.67 % | 2.770 → 2.682 s | −3.18 % |
| `route_planner/stuck` | 462.3 → 405.5 | −12.2 % | 108.8 → 95.09 ms | −12.6 % |
| `route_planner/yield_place` | 44.41 → 39.85 | −10.4 % | 10.32 → 9.342 ms | −9.43 % |
| `route_planner/priced` | 228.0 → 209.6 | −7.97 % | 51.92 → 49.03 ms | −5.55 % |
| `crowd_costs/update_1000_changes` | 2.477 → 0.9175 | −63.0 % | 513.2 → 183.0 µs | −64.3 % |
| `crowd_costs/update_10_changes` | 0.0557 → 0.0435 | −21.9 % | 13.47 → 10.72 µs | −20.5 % |
| `body_grid/refresh_still`, then `rebuild_same` | 0.0913 → 0.0146 | −84.0 % | 19.75 → 3.753 µs | −81.0 % |
| `body_grid/rebuild` | 0.0914 → 0.0909 | −0.57 % | 19.70 → 19.90 µs | +0.76 % |
| `route_planner/line` | 5.339 → 5.368 | +0.71 % | 1.109 → 1.129 ms | +1.74 % |
| `areas/mean_fields` | 935.5 → 948.4 | +1.48 % | 228.2 → 230.0 ms | +0.81 %, one pair +0.05 % |
| `stats/worst_auras` | 16.97 → 17.65 | +3.26 % | 3.047 → 3.088 ms | +1.09 % |
| `collision/spread` | 0.7458 → 0.7532 | +0.57 %, pairs both ways | 161.4 → 163.8 µs | +1.42 % |

The other cases of the final A/B, the capabilities' scenes, collision, steering, body index, the script view and the 3v3's other stages: on `mac` within ±1 %, but `server_frame/worst_1v1` +1.16 % (pairs +0.07 to +1.80 %), and `script_view/read` −2.16 %, `body_index/update` −1.70 % and `steering/steer` −1.36 %; on `local` within ±1.6 %, or with pairs both ways, as `script_view/reread` +3.07 % and `navigation/worst_open` +5.81 %, but `body_index/near` +3.43 %, `acquire/idle` +2.03 % and `abilities/mean_casts` +1.23 %, which the bisect above puts down to code placement.

## Session of 2026-10-25, the benches not yet covered

From `9de47b915`, which upgraded Rhai to 1.27.0, over the benches the first session did not measure: `math`, `sim`, `protocol`, `script`, the `net-*` crates, the net harness, and the harness' load, build and view cases. Hosts and method as above.

### First numbers

`local`, one run each, a build running beside some of them:

| Case | Time |
| --- | ---: |
| `sim`: `state_hash/all`, `snapshot/all`, `snapshot/restore`, `state_delta/changes`, `state_delta/apply` | 101 µs, 103 µs, 414 µs, 3.9 µs, 8.3 µs, over 1,000 units |
| `protocol`: `input_chain/sign`, `input_chain/check`, `session_log/record_tick_8_players`, `record_tick_32_players` | 14.1 µs, 19.9 µs, 164 µs, 723 µs |
| `net-harness`: `server_frame/mean_walk_3v3`, `client_frame/mean_walk_no_rollback_3v3`, `mean_walk_3v3`, `mean_walk_rollback_3v3`, each over 400 frames | 179 ms (448 µs a frame), 31.8 ms (79 µs), 48.1 ms (120 µs), 75.1 ms (188 µs) |
| `net-replication`: `send_insert_1_client`, `send_insert_16_clients`, `send_update_1_client`, `receive_insert_1_client`, `receive_update_1_client`, over 1,000 entities | 96 µs, 971 µs, 79 µs, 172 µs, 50 µs |
| `net-messages`: `send_receive_0_messages`, `_1000_`, `_10000_` | 19.4 µs, 120 µs, 859 µs |
| `net-udp`: `udp_server/frame_4096_links`, `admission/flood` | 197 µs, 465 µs |
| `math`: `vec3/step_toward`, `vec3/normalized`, `direction/of`, `num/atan2` | 200 µs, 128 µs, 89.5 µs, 55.7 µs, each over 4,096 inputs |
| `net-interpolation`: `interpolation/worst_moving`, `mean_moving` | 229 µs, 23.0 ms over 200 frames |
| `harness`: `session_log/encode_3v3`, `decode_3v3`, `package_load/3v3`, `match_build/3v3`, `script_compile/3v3`, `script_bind/3v3`, `script_view/read`, `reread` | 40.8 µs, 12.7 ms, 3.99 ms, 4.31 ms, 777 µs, 191 µs, 78.1 µs, 32.9 µs |
| `zero_hour_stage/mean_bases_*`, the stages of the bases play's 6,000 ticks on the tip | move 736 ms, collide 328 ms, hit 149 ms, inputs 76 ms, resolve 66 ms, think 50 ms, act 46 ms, mode 24 ms, vision 17 ms: about 249 µs a tick, move and collide 71 % of it; the bots' think, `zero_hour_bots/mean_bases`, 12.3 ms |

### Ideas

| Idea | Evidence | Numbers | Verdict |
| --- | --- | --- | --- |
| Rhai 1.27.0 against 1.26.1 | the script host's cases ran 18 to 43 % slower than the criterion baseline of the first session, before the upgrade | `local`, quiet, the candidate on 1.26.1 against `9de47b915` on 1.27.0: `script_host/native` −9.00 %, `native_handle` −8.74 %, `call` −0.22 %, `handle` +0.34 %; `battle/crowded_think` −3.53 %, `mean_3v3_think` −3.35 %, `mean_3v3_hit` −1.63 %, `mean_3v3_resolve` −1.87 %, `server_tick/mean_3v3` −1.81 %, `script_bind/3v3` −3.89 %, each pair the same way; `worst_3v3` +0.16 %, `script_compile/3v3` −0.30 % | 1.27.0 costs a native call 9 to 10 % more: the 3v3 tick 1.8 %, its think stage 3.4 % |
| The state hash's encoding: fixed-width values in place of postcard's varints | `state_hash/all`: `Num`'s postcard serialize 18.2 %, `StableId`'s 9.1 %, BLAKE3 13.8 % | | not tried: production hashes only at checkpoints and at the result (`TickHashes`), and the bytes define the hash, so a change is a format change for no tick's gain; only the tests' every-tick hash would run faster |
| Rhai 1.27.0 against 1.26.1, `mac` | | three pairs, the candidate on 1.26.1: `script_host/native` −5.71 %, `native_handle` −3.72 %, `call` −2.79 %, `handle` +0.13 %; `battle/crowded_think` −4.18 %, `mean_3v3_think` −2.85 %, `server_tick/mean_3v3` −1.97 %, each pair the same way; `worst_3v3` −0.51 % | confirmed on both hosts |
| The server's frame outside its sim tick | `server_frame/mean_walk_3v3` 448 µs a frame against about 86 µs a sim tick; `perf record` of the case: `bevy_replicon::server::collect_changes` 5.3 %, Bevy's executor and its run conditions, deferred commands and schedule lookups about 7 % | the harness runs the server and its clients in one process and one timed closure, so no sample tells the server's share from a client's | not tried: a server-alone profile needs a bench of the server's frame with its clients in another process |
| The session log's decode | `session_log/decode_3v3` 12.7 ms against 40.8 µs to encode: the decode records each entry again, checking each chain link and Schnorr signature, about 20 µs each | | not tried: a verifier's path, outside every tick |
| Signature checks off the tick's thread | `record_tick_8_players` 164 µs, each packet's Schnorr check about 20 µs | | not tried: `signature-checks.md` measured it at 0.45 % of a core at 8 players and 1.9 % at 32, and the protocol checks each packet as it records it |
| The restore | `snapshot/restore` 414 µs, four times a snapshot: `StateRegistry::put_in` 12.2 %, then decoding and the compare | | not tried: a restore runs at a checkpoint, a replay or a verification, never in a tick |
| The route planner's move stage by a flow field for a group | `zero_hour_stage`: move and collide 71 % of a Zero Hour tick | | not tried: navigation.md already plans one route a group (One command, one route) and refuses a flow field of the whole map each tick (A route prices the units on it), so the move stage's searches are single replans |
| `integration/replication/send_insert_0_clients` | 432 µs against 96 µs with one client: with no client, `Stepper::init` returns before any step, so the timed update is the server app's first, with its setup | | a fault of the bench, in the issue log |

## The counted fog, 2026-10-25

`master` (`6f56e7970`) against the branch `counted-fog` with the counted fog of `PLAN.md`'s F2, interleaved pairs, `local` in clock-scaled megacycles, quiet, four pairs; `mac` in ns, three pairs. Both trees' `fog/sight` moves each unit a whole cell along x and back, where it moved a bit before, so that every seer's cells change.

| Shape | `local` | `mac` |
| --- | --- | --- |
| Each moved look's old runs found on the grid again from its key | `fog/sight`, a bit's move, +67.4 %; `fog/still` −75.2 %; `battle/crowded_vision` −55.7 %; `mean_3v3_vision` −15.9 % | |
| The runs kept from the tick before, in two flat buffers | `fog/sight`, a bit's move, +4.9 %; `fog/still` −70.7 %; `mean_3v3_vision` −27.6 % | |
| And no count when a look's runs are the same | `fog/sight`, a bit's move, −26.1 %; a cell's move +111 % | |
| And a count cell by cell, `SightMaps::look` inlined | `fog/sight`, a cell's move, +78.6 %; `fog/still` −70.8 %; `battle/crowded_vision` −49.3 %; `mean_3v3_vision` −28.0 %; `server_tick/mean_3v3` −2.26 %; `server_tick/worst_3v3` −0.79 %; `worst_3v3_vision`, eight pairs from −22.8 % to +32.5 %, median +9.5 %; `zero_hour_tick/mean_attack` +0.98 %, `mean_bases` +0.30 %, `worst_attack` +0.09 %, `worst_bases` +2.74 %, three pairs | `fog/sight` +94.9 %; `fog/still` −65.5 %; `battle/crowded_vision` −47.9 %; `mean_3v3_vision` −28.0 %; `server_tick/mean_3v3` −2.60 %; `server_tick/worst_3v3` +0.82 %; `worst_3v3_vision` −12.6 % |

The profile of the last shape's `fog/sight` on `local`: `CountedMaps::count` 33.6 % and `SightMaps::look` 10.3 % before the count went cell by cell; the stage's spans, its diff and its copy of the runs cost about what the whole bitmap stage costs.
