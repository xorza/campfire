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

## The lead of a first order

The lead of each bot over the server from the start or the restart of its match to its first order, read on 27 October 2026 from the `lan-check-*` artifacts of CI run 40400460466, on commit `b1086867`, the first run after the frames' logs of every process were kept whole, which passed on macOS, Linux and Windows; and from 52 LAN check runs of 26 October 2026 on the same code, 30 on the Linux workstation and 22 on an M5 Mac, all passed. A bot's lead at a frame is its tick at the frame's start less the server's place then: the first tick of the server's latest frame, plus the time since that frame's start at 30 ticks a second. An order's margin is the time from its send to the start of the server's first frame that runs its stamp.

- **The lead at a first order is about 1 tick, where the objective is 2.** In the local runs, the last frame before a first order led by under 1.5 ticks in 54 of 106 orders on Linux and 44 of 78 on the Mac, at a median of 0.99 and 0.98, the least −0.01 and −0.22; 81 orders went in a bot's first frame, of no lead measured. In run 40400460466, the frames of both bots before their first order of the start led by a median of 0.96 to 0.98 ticks on every system.
- **A margin of a lead of 1 is the phase to the server's next tick.** The first orders' margins in the local runs: at least 11.5 ms on Linux, median 32.4 ms; at least 2.4 ms on the Mac, median 33.1 ms; none late. In run 40400460466: 17.5 to 31.2 ms for the orders stamped 20 on Windows and 25.3 ms on Linux.
- **On macOS a shared stall leaves no margin.** In run 40400460466 on macOS, the server and both bots stopped together for 60 to 110 ms four times in the first 0.6 s, at 0.05, 0.26, 0.43 and 0.55 s from the server's first frame, and woke within 2 ms of each other. Both bots began a frame at tick 19 at 0.5871 s, after the server's frame of tick 18 at 0.5858 s, and sent their orders stamped 20 at 0.5874 s; the server began its frame of tick 20 at 0.5883 s and logged both in tick 20 at 0.5888 and 0.5890 s, a margin of 0.9 ms. The bots' frames before it led the server by −1.27 to 3.91 ticks, median 0.98 and 0.96.

- **With the band ahead of the objective.** In 7 local LAN check runs of 27 October 2026 on the Linux workstation, once the input timeline stepped every deficit up to its objective, every run passed; in the 3 whose logs the driver kept, 15 first orders, the last frame before each first order led by 1.97 or 1.98 ticks, two earlier frames by 0.91 and 1.00, and the margins were 33.6 to 62.4 ms, median 50.0 ms, where the 30 Linux runs of 26 October gave 11.5 ms at the least and 32.4 ms at the median.

## How other engines keep an input early

Read on 27 October 2026.

- **Netcode for Entities.** The server measures how long it holds each client's commands before it uses them, and the client changes its tick's pace by small steps so that this age stays near `TargetCommandSlack`, 2 ticks by default; `CommandAgeCorrectionFraction`, 0.075 to 0.2, sets how much of the error a frame corrects ([Time synchronization](https://docs.unity3d.com/Packages/com.unity.netcode@6.5/manual/time-synchronization.html), [ClientTickRate](https://docs.unity3d.com/Packages/com.unity.netcode@1.4/api/Unity.NetCode.ClientTickRate.html)).
- **Overwatch.** A client runs ahead of the server by half the round trip and one command frame, and the server, as it finds its buffer of a client's inputs starved, tells the client to run slightly faster until the buffer fills, as summaries of Tim Ford's GDC 2017 talk give it; the talk itself was not read ([Edgegap](https://edgegap.com/blog/game-backend-deep-dive-overwatch-2016-netcode-architecture-rollback), [GameDev.net](https://gamedev.net/forums/topic/696756-command-frames-and-tick-synchronization/5386794/)).
- **The engine's own.** The input timeline's objective on loopback is 2 ticks ahead of the server, one for an input to reach the server before the tick that reads it and one for the sync's deadband of 1 tick, symmetric about the objective (`SyncSetup::error_margin`), so a lead of 1 is within the band and is not corrected; the timeline steps up to the objective only past the band (11-net-stack/time.md, An input leads by one margin for each reason; A client behind steps, a client ahead slews).
