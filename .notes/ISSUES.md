# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Client.** A player's own unit hides a round of its clip at the tick its client predicts the shot (`SlotAttack::lack`), and plays the shot's fire effect then, while the projectile the server spawns is drawn only once the client receives it, so a Tomahawk's missile leaves its rail a moment before the flying missile shows (report of 2026-10-26, local match of session `b89c796d…`, no F12 report). The client predicts its own units' slot times, and the projectile is sent once by the server; how long the gap lasts, and whether the projectile is drawn on the interpolation timeline, is not measured.
- **Net.** **Plan: N3.** On macOS CI run 40328962123, the LAN check's `lan.2` failed: the input of player slot 1 stamped 20 took effect in tick 21. Bot 1 sent it at 14:08:11.826 and the server logged it at 11.829, after the server ran ticks 17 to 19 in one frame, logged at 11.824, and tick 20 alone in the next frame; the check excuses a moved input only when its stamp lies within the ticks of a frame that ran several; [LAN late input](../docs/research/lan-late-input.md) holds earlier runs of the same failure.

## Ready

- **Client.** A turret's node does not turn where its model's state plays a clip that animates that node: the client writes the turret's rotation only when the aim changes (`UnitModelsPlugin::turn_turrets`), and Bevy's animation player writes the clip's rotation over it every frame, where W3D applies a turret's captured bone after the animation (`PivotClass::Capture_Update`). In the import of 2026-10-26, 79 of the 197 looks of a turret have such a clip, as the Overlord's `nvovrlrd` animates `turret01`; the sim's aim is right, as a replay of session `b89c796d…` shows the Overlord's aim turn from 0° to 160.9° before its first hit.


