# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### E5. Particles draw over the terrain's overlays

Touches the client's effects and terrain view, as a player's report showed an explosion cut into squares at Tournament Desert's road (2026-10-26, report `79957399…-2081`); it stands before the slice's end.

- [ ] The terrain's overlay mesh and each particle group's mesh are meshes of Bevy's transparent phase, which orders them by their bounds' centres and their depth bias, so the overlay, one mesh over the whole map, draws over the particles where its cells hold a blend or a decal, as squares of the cells' size cut out of an explosion; and each group's mesh starts with no vertex, of which Bevy's mesh allocator warns 40 times as the client starts. The particles draw after the terrain's overlays at any camera, and a group's mesh holds no buffer while it holds no quad. Direction: one rendering pipeline for every game. Checks: the client's tests draw a particle over an overlay cell after the overlay; a match logs no warning of an empty mesh.

### End of the slice

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them ([question](PLAN_QUESTIONS.md#effects-end-of-the-slice)); then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Missiles

Three faults of the Tomahawk a player's report showed (2026-10-26, report `79957399…-2081`, its replay probed at 100 ms): each of its four missiles ended 9 ticks, 0.3 s, after its launch, about 50 m out of its 370 m to the ECM tank it aimed at, which kept its 300 of 300 life; each launched from the launcher's place; and the launcher's model kept its missile on its rail after each launch. The steps stand after Effects, whose trails and ends the missiles carry.

### M1. A missile flies to its target

Touches the projectiles' steered flight; the cause is to find first.

- [ ] A Tomahawk missile ends with no hit 9 ticks after its launch, where its fuel of 4,000 ms lasts 120 ticks and its target lives: the steered flight ends at a lost target, at its fuel's end, outside the world's bound, or on a planar map's ground (`flights.rs`, `steered`), and the step finds which one ends it, by a replay of the report that logs each end's cause, then fixes it against the game's `MissileAIUpdate`. Its fuel's end differs from the game's too: the game stops a missile's thrust and turn at `FuelLifetime` and lets it fall, or detonates it with `DetonateOnNoFuel` (`zero-hour-missiles.md`, Fuel), where the engine ends it where it stands (`04-capabilities/actions.md`, Fuel ends every steered flight). Direction: a capability serves every game. Checks: the projectiles' tests fly a steered projectile of a cruise height and a dive to its target; the Zero Hour check's missile play fires a Tomahawk at a target 350 m away and pins its hit.

### M2. A projectile launches from its weapon's launch bone

Touches the import's weapons and looks, the actions' launch and the projectiles; it stands after M1.

- [ ] A projectile starts at its source's place, where Zero Hour starts it at the bone its state's `WeaponLaunchBone` names for the weapon's slot, turned with the unit and its turret (`Weapon::calcProjectileLaunchPosition`), which the logic reads of the model, so a launch is a fact of the data. The import writes each slot's launch offsets from its model's bone of the default state, and a launch starts at the offset, turned as its unit and its turret stand. Direction: what a module adds is declared once. Checks: the import's tests write a slot's launch offset from a model's bone; the actions' tests launch a projectile at a turned unit's offset.

### M3. A launcher's model hides the missile it launched

Touches the import's looks and the client's look states; it stands after M2.

- [ ] A Tomahawk launcher draws its missile on its rail while its weapon reloads, where Zero Hour hides each launch bone's sub-object of a slot of `ProjectileBoneFeedbackEnabledSlots` as the clip's rounds run out, and shows it again as the clip reloads (`W3DModelDraw::updateProjectileClipStatus`). The import writes each such slot's launch bones on the look, and the client hides one for each round the clip lacks, from the clip it derives (02-engine-core.md, The client derives an attack's phase). Direction: a look is states of conditions, and the client derives what the sim does not send. Checks: the import's tests write a launcher's feedback bones; the client's tests hide a bone after a shot and show it after the reload.

## Changed


- E3, shots, hits and deaths play their effects: a projectile's hit and trail are named on the projectile type's own look, as its end and its trail, since the client knows a projectile's type and not the action that fired it; no match test plays the triggers, whose parts, the run's play of a list and the resolution of the looks' names, are tested.

## Foundations
