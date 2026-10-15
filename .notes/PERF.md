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
