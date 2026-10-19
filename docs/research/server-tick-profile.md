# Server tick profile

Where the MOBA 3v3's mean server tick spends its time, sampled 2026-10-18 at `80dc76d07`.

## Machine and method

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, a laptop on mains power, with other work running.
- **Build:** the bench profile with frame pointers in its own target directory (02-engine-core.md, Frame pointers for profiling).
- **A run:** `perf record -F 2000 -g --call-graph fp` of `integration/server_tick/mean_3v3` with `--profile-time 20`; 40,310 samples. The shares below are of every sample, the bench's setup of each match among them; the timed ticks hold 68 % of them.

## Results

| What | Share of the samples, with its callees |
| --- | ---: |
| The ticks the bench times | 68.4 % |
| The AI's think batch | 26.9 % |
| Rhai's calls of script functions | 25.4 % |
| Script view reads | 11.6 % |
| Vision's system that reads the relations through `OnMatch` | 6.6 % |
| The damage pass, its script batch among it | 4.5 % |
| `ScriptCall::begin`, the check of the tick's draws at each call | 1.9 % |
| `Filter::take_text`, a filter's text parsed at each call | 1.5 % to 2.5 % |
| `PlayersCall::begin`, the players' resources copied at each call | 0.35 % |

- No lookup of the entity index's B-tree for the match's or a player's entity shows on its own: each is under the 0.5 % of a line the report shows, so the index's lookups cost little next to the work around them.
- The damage pass's own reads of the units are 0.13 % of the samples; the rest of its 4.5 % is its script batch.

## The two executors

The MOBA 3v3's server tick on Bevy's single-threaded and multi-threaded executors, measured 2026-10-18 at `606f9318c`.

- **Machine:** as above, the compute pool on its 32 threads.
- **Method:** `./drive.py bench -- --exact` of each case below, once with the sim schedule as it is, once with `SimUpdate::schedule` setting `MultiThreadedExecutor` in place of `SingleThreadedExecutor`, a change not kept. Criterion's 10 samples; the middle of its interval below, the interval after it.

| Case | Single-threaded | Multi-threaded | Ratio |
| --- | ---: | ---: | ---: |
| `integration/server_tick/mean_3v3`, a match of 6,000 ticks | 483.43 ms (477.45–487.10) | 609.24 ms (603.88–615.64) | 1.26 |
| `integration/server_tick/worst_3v3`, the worst tick | 453.96 µs (434.11–475.92) | 644.76 µs (559.58–760.44) | 1.42 |

- The mean tick is 80.6 µs on one thread and 101.5 µs on the pool.

## A parallel move pass

The move stage with `Walking::move_units` as a pass over its query on Bevy's compute pool, `par_iter_mut`, measured 2026-10-18 at `4adb7ba98`.

- **Machine:** as above, the compute pool on its 32 threads, made by `ComputeTaskPool::get_or_init(TaskPool::new)` as the sim builds its schedule, and `bevy_ecs`'s `multi_threaded` feature named.
- **Method:** `./drive.py bench -- --exact` of each case below: the base twice, in a worktree at the commit, and the parallel pass once, a change not kept. Criterion's 10 samples; the middle of its interval. The machine's load average was 2 to 4.
- **Result:** the parallel pass holds each unit's result to the same bits, and is slower in each case.

| Case | Base, first run | Base, second run | Parallel |
| --- | ---: | ---: | ---: |
| `integration/battle/crowded_move` | 5.62 ms | 6.55 ms | 7.20 ms |
| `integration/server_stage/mean_3v3_move` | 41.62 ms | 46.25 ms | 82.83 ms |
| `integration/server_stage/worst_3v3_move` | 167.10 µs | not run | 410.66 µs |

- Over the 3v3's 6,000 ticks, the parallel pass adds 41.21 ms to the first base, 6.87 µs a tick, where the base's move stage takes 6.94 µs a tick.
