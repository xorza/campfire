# LAN late input

Why the LAN check failed on macOS in CI run 39052741501, on commit `f3ad15c8`, with the inputs of slots 0 and 1 stamped 300 taking effect in ticks 301 and 303. Read on 20 October 2026 from the run's `lan-check-macOS` artifact, the server's and the bots' JSON logs after the server's restart. The run was on the stack before Campfire's own: QUIC links and Lightyear's timeline sync.

## What the logs show

- **The runner stalled every process together.** At 17:26:53.711 the server's frames stop for 0.54 s, in gaps of 0.148, 0.213 and 0.178 s, bot 0's for 0.538 s and bot 1's for 0.586 s.
- **The server caught up at once.** It ran ticks 222 to 226, 227 to 232 and 233 to 238 in three frames.
- **The bots came back behind the server.** Each bot's tick less the server's at the same wall time, sampled each 0.25 s: bot 0 led by +5 to +8 before the stall and stood at −6 after it; bot 1 led by +2 to +3 and stood at −14, then −7. Bot 0 then gained about one tick each 0.5 s: −4, −3, −2, −1, and 0 at 17:26:57.28. Bot 1 was at −3 to −2 until it left.
- **The late inputs.** Bot 0 sent its order stamped 300 at 17:26:56.331 and the server logged it 16 ms later in tick 301; bot 1 sent its own at 17:26:56.396, logged 18 ms later in tick 303. Each was sent while its bot's lead was −1 and −3, 2.6 s after the stall.

## The sync's speed

`SyncConfig`'s defaults in `lightyear_sync` 0.30.1, which the run used, and `SyncSetup`'s in `net-sync` now, the same values and formula: an error past `max_error_margin`, 10 ticks, snaps to the objective; an error past `error_margin`, 1 tick, three times in a row on one side, changes the speed to `1 + 0.05 × 2 × error / 10`. An error of 6 ticks runs the timeline 6 % fast, 1.8 ticks a second at 30 Hz, and the speed falls with the error, so the error decays with a time constant of 3.3 s.

## Run 39659975702: a client's catch-up frame

Why the LAN check failed on macOS in CI run 39659975702, on commit `3dbdba08`, with the input of seat 1 stamped 20 taking effect in tick 21. Read on 22 October 2026 from the run's `lan-check-macOS` artifact, run directory `2026-10-21T19-32-32.597Z`, every time on the one host's clock.

- **The runner stalled every process together.** The server and both bots woke within 2 ms of each other after waits of at most one 33 ms tick, three times: at 33.459, at 33.739 to 33.744 and at 33.859 to 33.861 s.
- **The server ran tick 20 on time.** It caught up ticks 17 to 19 in its frame of 33.860 s, then read its inputs and ran tick 20 at 33.861 s.
- **The bot ran three ticks in one frame.** Bot 0, seat 1, began its frame of 33.861 s at tick 19 after a gap of 117 ms and ran ticks 19, 20 and 21 in it. It stamped its order for tick 20, the frame's second tick, and sent it at the frame's end, 0.34 ms after the frame began and 0.2 ms after the server had read its inputs for tick 20.
- **The server moved the order to tick 21.** It logged the input at 33.864 s, in tick 21. Had the order been stamped for tick 21, the frame's last, it would have reached the server at 33.8611 s, before its read for tick 21 at 33.8636 s.
- **The check excused no stall but the server's.** The server's catch-up covered ticks 17 to 19 and not the stamp, 20; the bot's own catch-up left no event in its log.

## Run 39778086754: a lead of no tick

Why the LAN check failed on macOS in CI run 39778086754, on commit `42d4a2ea`, with the input of seat 1 stamped 302 taking effect in tick 303. Read on 22 October 2026 from the run's `lan-check-macOS` artifact, run directory `2026-10-22T13-11-45.714Z`, after the server's restart.

- **The runner stalled every process together.** The server's frames stopped from 56.672 to 56.837 s, 164 ms, about 5 ticks; bot 0's from 56.671 to 56.835 s and bot 1's from 56.692 to 56.840 s.
- **The server woke first.** Its frame of 56.837 s ran ticks 298 to 301, and its frame of 56.840 s ran tick 302. Bot 1 began its frame at 56.8396 s at tick 298, ran 298 to 301 in it, and sent its order stamped 302 at 56.8406 s, 0.8 ms after the server read its inputs for tick 302.
- **The bots led by no tick.** Each bot's tick at the start of its frame, less the server's at its latest frame's start, over the whole match after the restart: bot 1 at 0 in most frames, 1 at most before the stall; bot 0 between 0 and 1, with excursions from −2 to 4. In the Linux run of the same commit, every frame of both bots led by 1 or 2.
- **Bot 0's order was in time.** It woke 5 ms before the server and stamped 303 in its catch-up frame, which the server logged for tick 303.

## Run 39812856711: a packet's tick behind its sender

Why the LAN check failed on macOS in CI run 39812856711, on commit `58576733`, with the input of seat 0 stamped 20 taking effect in tick 21. Read on 22 October 2026 from the run's `lan-check-macOS` artifact, run directory `2026-10-22T16-43-54.704Z`, before the server's restart.

- **The bots ran level with the server or behind it from the match's start.** The server began its frame of tick 0 at 56.1225 s and ran ticks 0 and 1 in it; both bots began their frames of tick 0 at 56.1278 and 56.1281 s. At 56.6837 s bot 1, seat 0, began a frame at tick 16 while the server ran 17 and 18.
- **The order missed by 0.4 ms.** Bot 1 began its frame of 56.7408 s at tick 18, 1.8 ms after the server's frame of tick 19, ran ticks 19 and 20 and sent its order stamped 20 at 56.7430 s; the server had begun its frame of tick 20 at 56.7427 s and logged the order in tick 21.
- **The server woke late and at random phases on macOS.** The 90th percentile of the gaps between the server's frames after its restart: 109.5 ms on macOS, 32.3 ms on Linux and Windows; median 3.46 ms, 1.08 ms and 1.83 ms; frames per tick 1.15, 2.49 and 2.23.
- **A packet carried the tick and not the instant.** The transport stamped each packet with the sender's tick, which its frame's end leaves up to a tick past, and the remote timeline took that tick as the sender's instant, so its estimate ran behind the sender by its mean overstep at its sends.
