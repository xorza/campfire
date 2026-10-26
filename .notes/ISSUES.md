# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Net.** **Plan: N3.** On macOS CI run 40328962123, the LAN check's `lan.2` failed: the input of player slot 1 stamped 20 took effect in tick 21. Bot 1 sent it at 14:08:11.826 and the server logged it at 11.829, after the server ran ticks 17 to 19 in one frame, logged at 11.824, and tick 20 alone in the next frame; the check excuses a moved input only when its stamp lies within the ticks of a frame that ran several; [LAN late input](../docs/research/lan-late-input.md) holds earlier runs of the same failure.

## Ready

- **Combat.** A turret turns toward its unit's attack target whatever weapon the unit attacks with (`Aiming::aim`), where Zero Hour's turret aims only while the current weapon is of a slot it controls (`AIUpdateInterface::getWhichTurretForCurrentWeapon`): a unit that attacks with a weapon its turret does not carry turns its turret toward the target all the same. No converted unit of the import of 2026-10-26 has such a weapon.

