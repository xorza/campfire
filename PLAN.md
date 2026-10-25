# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Animated looks

The slice of [Animated looks](docs/design/animated-looks.md): a player of a Zero Hour skirmish sees infantry stand, walk, fire and die with the original's animations, a building rise as it is built and show its damage, and an upgrade change what its units show. Four batches, each one commit; its checks are that document's.

### A3. Playing an animation (client)

- [ ] A look's state names no animation (`client-files/src/client_unit.rs`), and the client plays none → each state names its draw module's animations, and with Bevy's `bevy_animation` feature each unit's state plays its animation by its mode, start and speed, its idle animations as a loop ends, and a transition's animation between two states of its keys; a clip's tracks bind to each model of its hierarchy by its nodes' names (animated-looks.md, A state plays its animation by its mode).
- Checks: the client's tests give each mode's frame at a given time and choose the transition between two keys; in the client, a Ranger idles and walks.

### A4. Attacks, sites and upgrades (production, import, client)

- [ ] A site's progress reaches its owner alone (`production/src/site.rs:189`) → it reaches every client that holds the site, and a state may rise with it (animated-looks.md, A building rises with its site).
- [ ] The client reads no attack's phase for a look → the conditions of an attack's pre-attack, strike, wait between shots and reload of each weapon slot come from `ActionProgress` and the clip's state.
- [ ] `PowerPlantUpgrade`, `ModelConditionUpgrade` and `SubObjectsUpgrade` are dropped whole → each converts to a modifier of a tag its states test, its other effects still a loss (animated-looks.md, An upgrade's look is a tag).
- Checks: the client's tests compute each attack phase and a site's rise; the Zero Hour check's skirmish play pins a site's rise; in the client, a barracks rises, a Ranger fires, and a power plant shows its rods.

### A5. Deaths (combat, import, client)

- [ ] A dead unit despawns at the end of its tick (`combat/src/dying.rs:76`) → a type's death may keep it for a time, of no collision, target or order, and the import keeps each object for its `SlowDeathBehavior`'s `DestructionDelay` (animated-looks.md, A death keeps its unit for its animation).
- Checks: a scripted match keeps a dead unit its time and despawns it after, every other pass passing over it; the Zero Hour check's golden is written again; in the client, a Ranger dies with its animation.

### A6. The check and the fold (the Zero Hour check, documents)

- [ ] The Zero Hour check's skirmish play pins a Ranger's states as it walks, fires and dies, and counts the states converted and dropped by cause.
- [ ] `09-zero-hour.md`'s losses "Deaths" and "A site's look" lose what the slice converts and gain "Doors", and the slice's decisions move into `02-engine-core.md`, `09-zero-hour.md`, `04-capabilities/combat.md` and `04-capabilities/production.md`, and `animated-looks.md` is deleted.
- Checks: the slice's; it ends when a player sees its checks in the client.

## Foundations
