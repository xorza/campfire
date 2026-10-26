# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Looks of the report of 2026-10-26

Two faults a player's report showed (2026-10-26, report `1e38d2c4…-945` and its session's log to tick 5711, on Tournament Desert): a Humvee drew its gun's muzzle flash at all times; and China's Gattling tank, attacking a Crusader from tick 5505, did not spin its barrels. Each cause is found; L2 and L3 are one batch, as both change the import's looks and the client's look states.

### L2. An attacking unit draws its attacking state

Touches the client files' tests, the import's look flags and the client's look states; it and L3 are one batch, as both change the import's looks and the client's look states, and it stands first of the two, as it adds no data the client must time.

- [ ] `ATTACKING` is a condition of no test, `never`, so no unit draws a state of it: the Gattling tank's `ATTACKING` states loop its barrels' spin, and other looks name it beside their attack phases. Zero Hour sets it while its unit's AI attacks, from its attack state's start to its end, its approach and its pursuit among it (`AIAttackState::onEnter`, `onExit`). A condition test holds while its unit has an attack target, and the import gives `ATTACKING` that test. Direction: a look is states of conditions, and the client derives what the sim does not send. Checks: the client files' tests read the test; the import's tests give `ATTACKING` it; the client's tests draw an attacking state while a unit has an attack target and its default after.

### L3. A muzzle flash shows only at its shot

Touches the import's looks and the client's models and look states, in L2's batch.

- [ ] A state's default hides each weapon slot's muzzle flashes before its own hides and shows (`ModelNodes::hidden`), so a `ShowSubObject` of a turret shows the flash on the turret's child pivot: the Humvee's state `NONE` shows `Turret`, and with it `avhummer.muzzlefx01`, at all times. Zero Hour hides each flash every frame after the state's hides and shows, and shows it while its barrel's recoil starts (`W3DModelDraw::handleClientRecoil`). The import hides the flashes after the state's hides and shows, and keeps them out of an upgrade's shows and a clip's rounds; the client shows a barrel's flash while its recoil starts, for the frames the draw module's recoil data give. Direction: the importer takes a game's data as it is, and the client derives what the sim does not send. Checks: the import's tests keep a flash hidden under a shown turret; the client's tests show a flash at a shot and hide it after.

## Turret slots

A fix of the issue log's Ready items, found 2026-10-26.

### I2. A turret carries the slots its data names

Touches the combat capability's turret and the import's turrets, one batch; it stands after the looks, as no converted weapon of the import of 2026-10-26 stands in a slot its turrets do not control, so it changes no match of Zero Hour's.

- [ ] A turret carries every weapon of its slot kind, where Zero Hour's carries only the slots its `ControlledWeaponSlots` names, and aims the slots of an `AltTurret` with a second turret (`AIUpdateInterface::getWhichTurretForWeaponSlot`); the import reads only an AI module's first `Turret`. A turret carries the weapon slots its data names, and the import writes them from `ControlledWeaponSlots` and an `AltTurret`'s. No converted weapon of the import of 2026-10-26 stands in a slot its turrets do not control; the four `AmericaVehicleBattleShip` types fire one weapon from `PRIMARY` and `SECONDARY`, which their `AltTurret` aims. Direction: a capability serves every game. Checks: the combat tests aim a turret's weapon and fire a weapon of another slot at the body's facing; the import's tests write a turret's slots and an `AltTurret`'s.

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

It stands last, as the player's check of each shot's effect at its barrel waits for L3's muzzle flash.

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Foundations
