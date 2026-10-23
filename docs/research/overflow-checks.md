# Overflow checks

What the release profile's `overflow-checks = true` costs the sim's cases, measured 2026-10-22 on the `workspace-hardening` branch at `10a44573`.

## Method

- **Builds:** two worktrees of one commit, one with `overflow-checks = true`, the release profile as it ships, and one with `overflow-checks = false`; each built its benches through `cargo drive bench`, whose bench profile inherits the release's fat LTO.
- **Runs:** "on", then "off", then "on" again, one after the other on one host, an Intel i9-13980HX of 32 threads. The last minutes of the "off" run shared the host with a build of the workspace's lints and tests, which can only slow it, so a cost below reads low, never high.
- **Noise:** the two "on" runs of one build differ by up to 7 % (`server_tick/mean_3v3`, 489.84 ms then 456.87 ms), so a difference of a few percent between "on" and "off" is within it.

## Results

Each case's mean of Criterion's estimate, "on" from the second run, or the first where the second ran none.

| Case | On | Off | On less off |
| --- | --- | --- | --- |
| `integration/server_tick/mean_3v3` | 456.87 ms | 449.48 ms | +1.6 % |
| `integration/server_tick/worst_3v3` | 457.29 µs | 454.44 µs | +0.6 % |
| `integration/battle/crowded_inputs` | 3.340 ms | 3.224 ms | +3.6 % |
| `integration/battle/crowded_think` | 82.81 ms | 81.82 ms | +1.2 % |
| `integration/battle/crowded_act` | 13.77 ms | 13.69 ms | +0.6 % |
| `integration/battle/crowded_move` | 5.770 ms | 5.662 ms | +1.9 % |
| `integration/battle/crowded_collide` | 6.172 ms | 5.816 ms | +6.1 % |
| `integration/battle/crowded_hit` | 10.06 ms | 9.825 ms | +2.4 % |
| `integration/battle/crowded_resolve` | 29.36 ms | 28.62 ms | +2.6 % |
| `integration/battle/crowded_mode` | 658.6 µs | 602.0 µs | +9.4 % |
| `integration/battle/crowded_vision` | 8.752 ms | 8.425 ms | +3.9 % |
| `integration/collision/crowded` | 311.1 µs | 290.9 µs | +7.0 % |
| `integration/route_planner/walled` | 20.61 ms | 19.22 ms | +7.3 % |
| `integration/state_hash/all` | 90.99 µs | 91.02 µs | 0.0 % |
| `integration/snapshot/all` | 92.19 µs | 94.40 µs | −2.3 % |
| `atomic/num/mul` | 3.895 µs | 3.933 µs | −1.0 % |
| `atomic/num/mul_div` | 11.33 µs | 9.830 µs | +15.2 % |
| `atomic/num/sqrt` | 5.889 µs | 5.828 µs | +1.0 % |
| `atomic/num/sin_cos` | 27.98 µs | 28.13 µs | −0.5 % |
| `atomic/vec3/length` | 12.41 µs | 12.27 µs | +1.1 % |
| `atomic/vec3/normalized` | 122.1 µs | 121.9 µs | +0.2 % |

## Findings

- **A 3v3 tick costs about 2 % more with the checks:** the server's mean tick +1.6 %, its worst +0.6 %, both within the runs' noise.
- **The crowded battle's stages cost 1 % to 9 % more,** the mode's stage the most, and the collision and route cases about 7 %.
- **One arithmetic case pays the most:** `mul_div`, whose `i128` steps each take a check, +15 %. `mul`, `sqrt` and `sin_cos` pay none past the noise.
