# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Net faults

A fix of the issue log's Net items, found 2026-10-26. It stands first, as N2 makes each CI run after it keep the frames N3 measures.

### N3. The lead of a bot's first order is measured

Touches `docs/research/lan-late-input.md` and the issue log; it stands after N2, as it reads the frames N2 keeps.

- [ ] Every failure of `lan.2` the research holds, stamps 300, 20, 302, 20 and 20, is the first order after a start or a restart; in run 40328962123, bot 0 stamped 20 while the server's last tick was 19, a lead of 1 where the objective is 2, which the sync's deadband of 1 lets stay, and after a stall of every process a lead of 1 leaves only the phase to the server's next tick for the bot's frame and send. From the artifacts of macOS runs after N2, failed and passed, and of Linux runs for comparison: each bot's tick at each frame's start less the server's, from its start or restart to its first order; the frame each order left in; whether the lead reached its objective by then. The facts go to the research, dated; the issue moves to Decide with the options they show, as a deadband on the ahead side alone, or a lead the sync reaches before a first order may go. Checks: none of the code.

## Turret slots

A fix of the issue log's Ready items, found 2026-10-26.

### I2. A turret carries the slots its data names

Touches the combat capability's turret and the import's turrets, one batch; it stands before the end of the Effects slice, as no converted weapon of the import of 2026-10-26 stands in a slot its turrets do not control, so it changes no match of Zero Hour's.

- [ ] A turret carries every weapon of its slot kind, where Zero Hour's carries only the slots its `ControlledWeaponSlots` names, and aims the slots of an `AltTurret` with a second turret (`AIUpdateInterface::getWhichTurretForWeaponSlot`); the import reads only an AI module's first `Turret`. A turret carries the weapon slots its data names, and the import writes them from `ControlledWeaponSlots` and an `AltTurret`'s. No converted weapon of the import of 2026-10-26 stands in a slot its turrets do not control; the four `AmericaVehicleBattleShip` types fire one weapon from `PRIMARY` and `SECONDARY`, which their `AltTurret` aims. Direction: a capability serves every game. Checks: the combat tests aim a turret's weapon and fire a weapon of another slot at the body's facing; the import's tests write a turret's slots and an `AltTurret`'s.

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Foundations
