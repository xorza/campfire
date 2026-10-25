# Perf

The notes of the optimization sessions: how this project measures, each host's noise, the findings that still hold, and each idea rejected or not tried with its reason, so that no session tries one again without new evidence. Numbers are dated; what landed is in git and in the design it cites.

## Hosts and method

- **Hosts.** `local`: an Intel Core i9-13980HX, 8 P-cores with SMT (CPUs 0–15) and 16 E-cores (CPUs 16–31), governor `powersave` with EPP `performance`, 64 GB. `mac`: an Apple M2, 4 P-cores and 4 E-cores, 8 GB, which swaps on a fat-LTO link; no `perf`, so no arm64 profile.
- **Trees.** `../campfire-perf/` holds the worktrees `base`, `cand`, `bisect`, `spike`, `fog` and `targets`, and the tools: `build.sh <root> <jobs> "<trees>" <pkg:bench>…` builds one bench target at a time and writes a PID file in `pids/`; `ab.py <bench> <filter> <pairs> <host>`, `TREES=a,b` for others than `base,cand`, runs interleaved pairs; `quiet.sh stop|cont` pauses the builds by their PID files. The Mac holds a bare repository at `~/.cache/optimize-perf/campfire.git`, fed by `git push`, and the same trees.
- **One bench target a build.** A build of every bench at once, two trees in parallel, ran `local` out of memory; each tree builds one target at a time, `-j 16` on `local` and `-j 4` on the Mac.
- **A bench alone on its host.** A build on the E-cores beside a bench on CPU 12 moved single pairs by up to 450 % and medians by 10 %; a verdict comes from a quiet host.
- **Clock-scaled time on `local`.** Each run is scaled by its process's mean clock, `perf stat -e cpu_core/cycles/u,task-clock` (`cycles:u` names the E-cores' counter too), to megacycles an iteration; the Mac's runs are plain ns.
- **The median of interleaved pairs** decides ([Benches](../docs/design/02-engine-core.md#benches), A verdict is the median of interleaved pairs): the spread between two runs of one binary passes criterion's interval within a run many times over.
- **No fixed workload by instructions.** `Scale` sets each group's sample count and time, which outweigh criterion's command line, so `instructions:u` over a process measures no fixed work.
- **LBR call graphs on `local`:** `perf record --call-graph lbr` unwinds the bench build as it is; its stacks hold 32 branches, so a deep stack's inclusive share is a low bound.

## Noise floors

A/A, base and candidate of one commit, interleaved pairs, 2026-10-25:

| Host | Case | Floor |
| --- | --- | --- |
| `local`, quiet | `server_tick/mean_3v3` | ±1 to ±1.4 % |
| `local`, quiet | tight atomic cases, as `body_index/near`, `acquire/idle` | ±0.15 to ±0.75 % |
| `local`, quiet | `zero_hour_tick/worst_*` | ±6 %, and see below |
| `local` | `battle/crowded_act` | −25 to +12 %: unusable |
| `mac` | the cases above | ±0.5 to ±1.4 % |

## Findings that hold

- **The worst ticks under memory pressure (2026-10-25).** `zero_hour_tick/worst_attack` on `local`: `6f56e7970` against `ef3573c31` gave 20.5 against 30.5 Mcyc in every pair of two runs, then 59.3 against 57.5 to 59.6 a run later, of the same binaries, while their whole runs spent the same user cycles and `ef3573c31` fewer page faults and kernel cycles; the host held 9 GB free under 46 GB of page cache, after 5.3 million compaction stalls since its start. The Mac gave +0.75 %. A worst tick on `local` measures the kernel's stalls as much as the code while the page cache crowds the memory; confirm one on `mac`.
- **Rhai 1.27.0 costs a script tick (2026-10-25).** The tip on 1.27.0 against the same tip on 1.26.1, quiet, each pair the same way: `script_host/native` +9.0 % and +5.7 % (`local`, `mac`), `battle/crowded_think` +3.5 % and +4.2 %, `mean_3v3_think` +3.4 % and +2.9 %, `server_tick/mean_3v3` +1.8 % and +2.0 %; `worst_3v3` does not move. The workspace keeps 1.27.0.
- **Where a tick goes (2026-10-25).** A 3v3 match's stages, `local`: think 222 ms, move 60, vision 55, hit 52, resolve 51, collide 37, inputs 19, act 17, mode 8; 520 ms in all, 86.7 µs a tick, before the counted fog and the target classes. Zero Hour's bases play: move and collide 71 % of about 249 µs a tick. A net server's 3v3 frame costs 448 µs against its sim tick's 86 µs; the harness runs the server and its clients in one process, so its share cannot be split without a bench of the server alone.

## The counted fog

`6f56e7970` against the counted fog's shapes, interleaved pairs, 2026-10-25; both trees' `fog/sight` moved each unit a whole cell along x and back, where it had moved a bit ([Vision](../docs/design/04-capabilities/vision.md), A stage that costs what changed):

| Shape | `local` | `mac` |
| --- | --- | --- |
| A moved look's old runs found on the grid again from its key | `fog/sight`, a bit's move, +67.4 %; `fog/still` −75.2 %; `mean_3v3_vision` −15.9 % | |
| The runs kept from the tick before | `fog/sight`, a bit's move, +4.9 %; `mean_3v3_vision` −27.6 % | |
| And no count when a look's runs are the same | `fog/sight`, a bit's move, −26.1 %; a cell's move +111 % | |
| And a count cell by cell, the shape kept | `fog/sight`, a cell's move, +78.6 %; `fog/still` −70.8 %; `battle/crowded_vision` −49.3 %; `mean_3v3_vision` −28.0 %; `server_tick/mean_3v3` −2.26 % | `fog/sight` +94.9 %; `fog/still` −65.5 %; `battle/crowded_vision` −47.9 %; `mean_3v3_vision` −28.0 %; `server_tick/mean_3v3` −2.60 % |

## Rejected

| Idea | Why |
| --- | --- |
| `Regions::nearest`'s regions from a heap | its pops cost more than one sort when a scan visits many regions: `zero_hour_tick/worst_attack` +135 % |
| `probe_against` out of line | LLVM kept the probe's loop apart, through memory: `body_box/reach` +4.5 % |
| `keep_in_bounds` over changed positions | its read of `MoveStep` orders it against the stats refresh, for 0.12 % of a tick |
| Gather: waiting workers by node, sorted each run | `gather/two_fields` +3.75 % |
| Rhai's function resolutions kept from call to call, in a patched Rhai | the think stage −1.2 %, the 3v3 tick −0.2 %, inside its floor; a fork for it is not worth its upkeep |

## Not tried

| Idea | Why |
| --- | --- |
| Rhai's progress callback less often than each operation | 0.96 % of the 3v3's self time; Rhai offers no period, and a count by other steps moves where a script's budget ends |
| One spatial index of the units a tick, shared by the unit grid, the targets, the broadphase and the crowd costs | each indexes another set by another key; a redesign of its own |
| `UnitGrid` and `Broadphase` sorted from the tick before's order | 0.5 to 1 % of a Zero Hour play, below its floor |
| The view's rows' ids in a dense array kept by the read's merge | the lookup it speeds is at most 1.2 % of the battle |
| `DueRun` for `Thinking::think`; `plan_routes` by `Changed<Route>`; `want_passives`, `keep_in_bounds`, `StaticTracking`, `Tracking::stale` by filters | each at the floor of a scan of change ticks, below what a case shows |
| The state hash's encoding by fixed widths | production hashes only at checkpoints and at the result, and the bytes define the hash |
| The session log's decode; the restore; signature checks off the tick's thread | each outside every tick: a verifier's path, a checkpoint's, or 0.45 % of a core at 8 players (`signature-checks.md`) |
| A flow field for a group's move | `navigation.md` plans one route a group and refuses a flow field of the whole map |
| `Flying::lose_teleported` by target; `NavigationColumn::room_for` near the box; one restore target a replay | cold paths that no case runs |
| The client bench | its fat-LTO link swaps the Mac; the candidates of 2026-10-25 changed no code it runs |
