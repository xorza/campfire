# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### End of the slice

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Missiles

Three faults of the Tomahawk a player's report showed (2026-10-26, report `79957399…-2081`, its replay probed at 100 ms): each of its four missiles ended 9 ticks, 0.3 s, after its launch, about 50 m out of its 370 m to the ECM tank it aimed at, which kept its 300 of 300 life; each launched from the launcher's place and flew flat, where the game's leaves its rail near upright; and the launcher's model kept its missile on its rail after each launch. The steps stand after Effects, whose trails and ends the missiles carry.

### M1. A projectile launches from its weapon's launch bone, along it

Touches the import's weapons and looks, the actions' launch and the projectiles, as it fixes the cause of the missiles' early end. The cause, found 2026-10-26: on a planar map, a steered flight ends where its height over the ground falls below 0 (`flights.rs`, `steered`), and a missile launches at its source's ground, height 0, and flies its straight run toward its target's center, so a target on lower ground, or a rise of the ground on the way, puts it under the ground within a few ticks; a test of the same flight over level ground strikes its target 370 m away after 65 ticks.

- [ ] A projectile starts at its source's place, where Zero Hour starts it at the bone its state's `WeaponLaunchBone` names for the weapon's slot, turned with the unit and its turret (`Weapon::calcProjectileLaunchPosition`), which the logic reads of the model, so a launch is a fact of the data. The import writes each slot's launch offsets from its model's bone of the default state, and a launch starts at the offset, turned as its unit and its turret stand. A steered projectile also leaves toward its target (`04-capabilities/actions.md`, A steered flight leaves toward its target), so a Tomahawk flies flat from its rail, where Zero Hour's missile leaves along its launch bone's `x`, the rail's, raised by twice its target's rise over the ground's distance, then flies straight for its `DistanceToTravelBeforeTurning` (`MissileAIUpdate::projectileLaunchAtObjectOrPosition`), so a Tomahawk leaves near upright (a player's report, 2026-10-26). The import writes each slot's launch direction beside its offset, and a steered projectile leaves along it, turned as its unit and its turret stand, raised as the game raises it. Direction: what a module adds is declared once. Checks: the import's tests write a slot's launch offset and direction from a model's bone; the actions' tests launch a projectile at a turned unit's offset; the projectiles' tests start a steered flight along its launch direction, raised by its target's rise.

### M2. A missile falls as its fuel ends

Touches the projectiles' steered flight and the import's missiles; it stands after M1.

- [ ] A steered flight ends where it stands as its fuel runs out (`04-capabilities/actions.md`, Fuel ends every steered flight), where Zero Hour's missile stops its thrust and its turn at `FuelLifetime` and falls, its velocity kept and pulled by gravity, until it meets the ground or a unit, or detonates at once with `DetonateOnNoFuel` (`zero-hour-missiles.md`, Fuel), which the import drops. A steered type's flight falls past its fuel, by a gravity its data names, which the import writes as Zero Hour's. Direction: a capability serves every game. Checks: the projectiles' tests fly a steered projectile past its fuel to the ground at hand-computed steps; the Zero Hour check's missile play pins a Tomahawk's hit on a target 350 m away.

### M3. A launcher's model hides the missile it launched

Touches the import's looks and the client's look states; it stands after M2.

- [ ] A Tomahawk launcher draws its missile on its rail while its weapon reloads, where Zero Hour hides each launch bone's sub-object of a slot of `ProjectileBoneFeedbackEnabledSlots` as the clip's rounds run out, and shows it again as the clip reloads (`W3DModelDraw::updateProjectileClipStatus`). The import writes each such slot's launch bones on the look, and the client hides one for each round the clip lacks, from the clip it derives (02-engine-core.md, The client derives an attack's phase). Direction: a look is states of conditions, and the client derives what the sim does not send. Checks: the import's tests write a launcher's feedback bones; the client's tests hide a bone after a shot and show it after the reload.

## Changed


- E3, shots, hits and deaths play their effects: a projectile's hit and trail are named on the projectile type's own look, as its end and its trail, since the client knows a projectile's type and not the action that fired it; no match test plays the triggers, whose parts, the run's play of a list and the resolution of the looks' names, are tested.

## Foundations
