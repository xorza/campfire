# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Net.** **Plan: N3.** On macOS CI run 40328962123, the LAN check's `lan.2` failed: the input of player slot 1 stamped 20 took effect in tick 21. Bot 1 sent it at 14:08:11.826 and the server logged it at 11.829, after the server ran ticks 17 to 19 in one frame, logged at 11.824, and tick 20 alone in the next frame; the check excuses a moved input only when its stamp lies within the ticks of a frame that ran several; [LAN late input](../docs/research/lan-late-input.md) holds earlier runs of the same failure.

## Ready

- **Combat.** **Plan: I2.** A turret carries every weapon of its slot kind, where Zero Hour's turret carries only the slots its `ControlledWeaponSlots` names (`AIUpdateInterface::getWhichTurretForWeaponSlot`), and the import reads only an AI module's first `Turret`, where Zero Hour also aims the slots of its `AltTurret`: a weapon of another slot of a Zero Hour unit with a turret waits for the turret's aim, and its shots leave its barrels turned and pitched with the turret. In the import of 2026-10-26, no converted weapon stands in a slot its unit's turrets do not control; the four `AmericaVehicleBattleShip` types fire one weapon from `PRIMARY` and `SECONDARY`, which their `AltTurret` aims.
