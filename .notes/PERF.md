# Perf journal: change-driven passes

The journal of the optimization session on branch `perf-change-driven-passes`, from [optimization](optimization.md). Host: `local`, an AMD Ryzen 7 6800U (Zen 3+, 8 cores, SMT), governor `performance`. Runs are pinned with `taskset -c 12` on the bench binary; base and candidate interleave.

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

| Case | local |
| --- | --- |

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
| `Admission`: a map and an expiry queue | `admit` keeps the buckets in a `Vec` it walks twice a handshake | | not tried: `quic` has no bench target, and one needs `criterion` as a new dependency of the crate, which waits for the user |
| `Walking::keep_in_bounds`, `StaticTracking`, `Tracking::stale` | 0.12 % and 0.34 % of the 3v3 tick, each a scan of change ticks | | not tried: at the floor of a scan |
| Bisect of the batch of charges and toggles: the old `run_toggles` against the tip | the tip's profile gave `run_toggles` 0.52 % of the tick, against 0.46 % before | the old pass: `mean_3v3_inputs` +2.2 %, `crowded_inputs` +2.7 %, so the due run paid | kept the due run, then the variant below |
| `run_toggles` keeps the set of units with a toggle on, and finds new ones by the change ticks of `SlotTimes` alone | only a unit with a toggle on has one to keep, and a toggle turns on only as the unit's times change; the due run's scan read five change ticks a unit | against the due run: `mean_3v3_inputs` −5.9 %, `crowded_inputs` −2.9 % | kept |
