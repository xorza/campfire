# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Client.** A rollback that moves a predicted shot of a slot to a later tick gives the slot a ready tick past the one `FireSeen` holds, so the client takes it for a second shot: it plays the shot's fire effect and muzzle flash again and draws a second stand-in (`MatchEffects::play_fires`, `ShotStandIns::place`).
- **Net.** **Plan: N3.** On macOS CI run 40328962123, the LAN check's `lan.2` failed: the input of player slot 1 stamped 20 took effect in tick 21. Bot 1 sent it at 14:08:11.826 and the server logged it at 11.829, after the server ran ticks 17 to 19 in one frame, logged at 11.824, and tick 20 alone in the next frame; the check excuses a moved input only when its stamp lies within the ticks of a frame that ran several; [LAN late input](../docs/research/lan-late-input.md) holds earlier runs of the same failure.

## Ready



