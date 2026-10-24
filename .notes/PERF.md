# Perf journal: change-driven passes

The journal of the optimization session on branch `perf-change-driven-passes`. Host: `local`, an AMD Ryzen 7 6800U (Zen 3+, 8 cores, SMT), governor `performance`. Runs are pinned with `taskset -c 12` on the bench binary; base and candidate interleave.

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
| `rollback_scan.rs` scans only the pairs that gained an entry, with the anchor rule rewritten | `integration/rollback_scan/idle_400_check` against `idle_400_disabled`, 2026-10-24: a client predicting 400 still units and a mover, no misprediction | 50.7 µs against 42.7 µs a frame of both ends: the check 8.0 µs, about 20 ns a predicted unit, 40 µs for 2,000, 0.24 % of a 60 Hz frame; the Zero Hour check's plays run no client to profile | dropped: the scan does not show |
