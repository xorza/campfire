# Sparse-set markers

What sparse-set storage of the markers that toggle during play changes in the battle benches and the whole 3v3 tick, measured 2026-10-18.

## Machine and method

- **Machine:** Intel Core i9-13980HX, 32 threads, Linux 7.2, a laptop on mains power, with CI running remotely and no other build.
- **A run:** `./drive.py bench -- 'integration/battle|integration/server_tick/mean_3v3'`, criterion's defaults. A baseline with every marker in a table, at `d8f698218`, `--save-baseline table`; then one run for each marker with that marker alone as a sparse set, and a control run of the unchanged code, each `--baseline table`.

## Results

The change of each case's median against the baseline, less the control's, in %; a negative value is faster. The control's own change against the baseline is its row.

| Marker | mean_3v3 | inputs | think | act | move | collide | hit | resolve | mode | vision |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Control | −1.5 | −1.3 | −2.5 | −0.9 | −1.0 | −0.5 | −0.4 | −1.9 | −2.4 | +0.5 |
| `ForcedMove` | +1.2 | −1.9 | +2.6 | −0.7 | −1.8 | +0.1 | −2.0 | −0.9 | −1.7 | −1.1 |
| `Resetting` | +0.9 | −1.7 | +0.8 | +2.5 | −1.1 | −0.2 | −1.5 | −0.2 | +0.1 | −0.9 |
| `Kept` | +0.2 | −2.2 | +1.5 | +0.0 | −1.8 | +0.2 | −2.2 | +0.2 | −0.7 | −1.3 |
| `Respawn` | −0.1 | −1.9 | +1.4 | −0.4 | −0.8 | +0.3 | −2.7 | −1.4 | −1.6 | −2.2 |
| `AreaTrigger` | +0.3 | −2.1 | +2.8 | −0.7 | −1.0 | −0.1 | −1.8 | +0.2 | −0.2 | −2.1 |
| `OutOfPlay` | +0.1 | −1.3 | −0.4 | +2.7 | −1.5 | +0.8 | −2.0 | −0.6 | +0.3 | −1.4 |
| `HeldOut` | −0.3 | −1.1 | +2.0 | +0.1 | −0.9 | −0.4 | −2.2 | −0.6 | −0.5 | −1.7 |

- The unchanged code's control moved by up to 2.5 % against its own baseline, so a run's spread is about that.
- Each marker moved `inputs` and `hit` by about −2 % and `think` by about +2 % alike, which no one marker's storage explains: a drift between the runs, not a gain of a marker.
- No marker moved the whole 3v3 tick, `mean_3v3`, past 1.2 % either way.
