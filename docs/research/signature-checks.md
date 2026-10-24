# Signature checks on the server's frame

What the server's frame spends to check the players' packets, measured 2026-10-17 at `965f09d6e`.

## Machine and method

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, a laptop on mains power; the load average was about 10 from other work as the run started.
- **A run:** `./drive.py bench -- 'integration/session_log|atomic/input_chain'`, criterion's defaults. `record_tick_N_players` records one tick's packets into a log fresh from its header, one packet of one 8-byte input for each of N players, in slot order; the log's construction is outside the timing.

## Results

| Case | Time |
| --- | ---: |
| `atomic/input_chain/sign` | 12.4 µs |
| `atomic/input_chain/check` | 18.3 µs |
| `integration/session_log/record_tick_8_players` | 150.5 µs |
| `integration/session_log/record_tick_32_players` | 620.1 µs |

- A packet costs about 18.8 µs to record at 8 players and 19.4 µs at 32: its signature check, 18.3 µs, is 94 % to 97 % of it.
- At 30 ticks a second and one packet a player a tick, the checks take 4.5 ms of each second at 8 players, 0.45 % of one core, and 18.6 ms at 32, 1.9 %.
- The worst frame is one after a stall: it takes up to the max input delay less one ticks' packets, 9 under `InputRules::LAN`, so 9 × 620 µs, about 5.6 ms, at 32 players, a sixth of a 30 Hz tick.
