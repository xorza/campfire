# LAN late input

Why the LAN check failed on macOS in CI run 39052741501, on commit `f3ad15c8`, with the inputs of slots 0 and 1 stamped 300 taking effect in ticks 301 and 303. Read on 20 October 2026 from the run's `lan-check-macOS` artifact, the server's and the bots' JSON logs after the server's restart. The run was on the stack before Campfire's own: QUIC links and Lightyear's timeline sync.

## What the logs show

- **The runner stalled every process together.** At 17:26:53.711 the server's frames stop for 0.54 s, in gaps of 0.148, 0.213 and 0.178 s, bot 0's for 0.538 s and bot 1's for 0.586 s.
- **The server caught up at once.** It ran ticks 222 to 226, 227 to 232 and 233 to 238 in three frames.
- **The bots came back behind the server.** Each bot's tick less the server's at the same wall time, sampled each 0.25 s: bot 0 led by +5 to +8 before the stall and stood at −6 after it; bot 1 led by +2 to +3 and stood at −14, then −7. Bot 0 then gained about one tick each 0.5 s: −4, −3, −2, −1, and 0 at 17:26:57.28. Bot 1 was at −3 to −2 until it left.
- **The late inputs.** Bot 0 sent its order stamped 300 at 17:26:56.331 and the server logged it 16 ms later in tick 301; bot 1 sent its own at 17:26:56.396, logged 18 ms later in tick 303. Each was sent while its bot's lead was −1 and −3, 2.6 s after the stall.

## The sync's speed

`SyncConfig`'s defaults in `lightyear_sync` 0.30.1, which the run used, and `SyncSetup`'s in `net-sync` now, the same values and formula: an error past `max_error_margin`, 10 ticks, snaps to the objective; an error past `error_margin`, 1 tick, three times in a row on one side, changes the speed to `1 + 0.05 × 2 × error / 10`. An error of 6 ticks runs the timeline 6 % fast, 1.8 ticks a second at 30 Hz, and the speed falls with the error, so the error decays with a time constant of 3.3 s.
