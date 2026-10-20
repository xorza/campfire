# Change scans

What the passes that follow their parts' changes by change ticks cost in the MOBA 3v3's mean server tick, measured 2026-10-20 on the `structural-hardening` branch, which master took as one change. It replaces the measurement of 2026-10-19, which took six of these passes and bounded their scans by their whole own samples.

## Machine and method

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, a laptop on mains power.
- **Build:** the bench profile with frame pointers in its own target directory (02-engine-core.md, Frame pointers for profiling).
- **A run:** `perf record -F 2000 -g --call-graph fp` of `integration/server_tick/mean_3v3` with `--profile-time 20`; 39,526 samples on the performance cores, of which the timed ticks hold 98.2 %. The shares below are of every sample on those cores.
- **The tick:** `cargo drive bench --exact integration/server_tick/mean_3v3` in the same build, Criterion's 10 samples: 500.89 ms (499.34–502.54) for a match of 6,000 ticks, 83.5 µs a tick.
- **The split:** `perf script --inline --max-stack=1` gives each sample's inlined frames. A pass's own sample is its scan when a frame under the system's function is the query's iteration, as `next`, `fold`, `filter_fetch`, `fetch` or `is_changed`; its body when no frame is under the function, the loop's own code; its work otherwise, as a system parameter's fetch or the work for a unit that changed.

## Results

| Pass | Its own | Scan | Body |
| --- | ---: | ---: | ---: |
| `Refresh::run`, the stats refresh at each edge | 2.58 % | 1.73 % | 0.13 % |
| `ActionPassives::want` | 0.95 % | 0.63 % | 0.18 % |
| `Routing::route_units` | 0.33 % | 0.17 % | 0.10 % |
| `PlayerGrants::want` | 0.27 % | 0.19 % | 0.04 % |
| `Holds::hold_charges` | 0.23 % | 0.12 % | 0.08 % |
| `ItemHolds::want_items` | 0.18 % | 0.06 % | 0.05 % |
| `AuraCarriers::track` | 0.17 % | 0.12 % | 0.03 % |
| `Walking::keep_in_bounds` | 0.13 % | 0.03 % | 0.07 % |
| `Dying::die` | 0.10 % | 0.04 % | 0.03 % |
| `Toggles::run_toggles` | 0.07 % | 0.04 % | 0.01 % |

- **The gathering's two:** `GatherLoop::despawn_empty_nodes` and `GatherLoop::tag_gatherers` have no sample: the 3v3 holds no resource node and no gatherer, so their queries match no table.
- **The refresh's scan** is the `extend` of the units its `Changed` and `Added` filters select, 96 % of its scan samples.
- **All ten:** the scans hold 3.15 % of every sample, 3.21 % of the tick's time, 2.68 µs a tick; with the bodies, 3.87 % of every sample, 3.94 % of the tick's time, 3.29 µs. Their whole own samples hold 5.01 %, 5.10 % of the tick's time. Each is the same in every tick, a walk of the units, not a peak of one tick.
