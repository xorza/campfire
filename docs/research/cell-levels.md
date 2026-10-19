# Cell levels

The ratio between the levels of `CellLevels`, measured 2026-10-19 at `ca0505012` with the ratio as one constant, `RATIO_BITS`.

- **Machine:** an i9-13980HX, as in [Server tick profile](server-tick-profile.md), its load average 1.2 to 3.8 for each case below.
- **Method:** `./drive.py bench -- --exact` of each case, one at a time; the middle of Criterion's interval. A second run of ratio 4 found its first three cases within 4 % of the first run's, and spread at 104.0 µs, where the first run's 167.7 µs had an interval from 155 to 182 µs; the table gives the second.
- **Scenes:** the collision cases are a Collide pass over `KernelScene::UNITS` bodies of 0.2 to 1.19 m; `boxes` makes the bodies that may not be pushed boxes, and `bridge` adds a 1,117 × 100 m box.

| Case | Ratio 2 | Ratio 4 | Ratio 8 |
| --- | ---: | ---: | ---: |
| `integration/collision/spread` | 83.6 µs | 104.0 µs | 59.3 µs |
| `integration/collision/crowded` | 310.1 µs | 406.8 µs | 289.4 µs |
| `integration/collision/boxes` | 581.8 µs | 770.0 µs | 923.4 µs |
| `integration/collision/bridge` | 608.6 µs | 763.9 µs | 943.5 µs |
| `atomic/body_grid/rebuild` | 16.1 µs | 16.7 µs | 17.4 µs |
| `atomic/body_grid/visit_near` | 1.134 ms | 0.786 ms | 0.717 ms |
| `integration/battle/crowded_collide` | 5.93 ms | 5.69 ms | 5.82 ms |
| `integration/server_stage/mean_3v3_collide` | 18.60 ms | 18.27 ms | 19.70 ms |
| `integration/server_tick/mean_3v3` | 450.9 ms | 460.4 ms | 450.9 ms |

- The ratio 4 column's last five cases are of its first run, as the load rose past 9 in the second.
- `battle/crowded_collide` is 60 ticks, and `mean_3v3_collide` and `mean_3v3` 6,000: ratio 2's tick of the 3v3 is 75.1 µs.
- Both body grid and broadphase visit only the levels that hold a body.
