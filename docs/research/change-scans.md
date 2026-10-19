# Change scans

What the passes that follow their parts' changes by change ticks cost in the MOBA 3v3's mean server tick, measured 2026-10-19 on the architecture redesign, uncommitted over `f16bca40`.

## Machine and method

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, a laptop on mains power, load average 1.3 at the start.
- **Build:** the bench profile with frame pointers in its own target directory (02-engine-core.md, Frame pointers for profiling).
- **A run:** `perf record -F 2000 -g --call-graph fp` of `integration/server_tick/mean_3v3` with `--profile-time 20`; 39,394 samples, of which the timed ticks hold 59.4 %. The shares below are of every sample.
- **The tick:** `./drive.py bench -- --exact integration/server_tick/mean_3v3`, Criterion's 10 samples: 452.04 ms (450.78–453.44) for a match of 6,000 ticks, 75.3 µs a tick.

## Results

| Pass | With its callees | Its own |
| --- | ---: | ---: |
| `Refresh::run`, the stats refresh at each edge | 3.30 % | 2.27 % |
| `ActionPassives::want` | 0.95 % | 0.78 % |
| `PlayerGrants::want` | 0.35 % | 0.30 % |
| `Holds::hold_charges` | 0.19 % | 0.17 % |
| `ItemHolds::want_items` | 0.18 % | 0.14 % |
| `AuraCarriers::track` | 0.16 % | 0.14 % |

- **The refresh's scan:** of `Refresh::run`'s own samples, 65 % are Bevy's query filter and iteration, `filter.rs`, `tick.rs` and `iter.rs`: the `Changed` and `Added` filters of its query, about 1.5 % of every sample, 2.5 % of the tick's time, 1.9 µs a tick.
- **The other five:** their own samples hold their scans of change ticks and their work for the units that changed, 1.5 % of every sample together, so their scans are at most 2.6 % of the tick's time, 1.9 µs a tick. Their source lines do not separate, as `perf report --symbol-filter` does not take a system's wrapped symbol.
- **All six:** the scans of change ticks cost at most 5 % of the mean tick, 3.9 µs of 75.3 µs. Each is the same in every tick, a walk of the units, not a peak of one tick.
