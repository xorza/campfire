# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Base defenses

A player builds a Patriot battery, a tunnel network and a Stinger site and sees each fire at an enemy that comes near, the site's three Stinger soldiers with missiles, each soldier back 30 s after it dies and the tunnel's two defenders 10 s after; and trains Technicals, each of one of three chassis, Inferno cannons and, after a Strategy Center, Tomahawks, and sees them fire ([proposal](docs/design/12-base-defenses.md)). The Scud launcher, which needs a science, the demo trap, which has no body, and the weapons of `NOT_AIRBORNE` stay dropped. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. Buildings on a circle (M)

`production`, `client`, `import`, 04-capabilities/production.md, 09. First: the Patriot battery, the tunnel network and the Stinger site need it to stand at all.

- **The build.** `BuildBook::build` (`production/src/book_builders.rs:206`) takes a building of a box or a circle body and refuses one of none; `Placement::passes` (`production/src/placement.rs:50`) tests the building's shape against the walls, the map's bounds and the bodies; the builder's walk (`production/src/construction/build_view.rs:269`) goes to the shape's nearest point; the client's preview (`apps/client/src/build_preview.rs:179`) draws and tests the circle. `ProductionDataError::NoBox` becomes `NoBody`. Builds: what can be forgotten is a type.
- **The import.** `ButtonDropCause::NoBox` (`apps/import/src/zero_hour/production_import/mod.rs:205`) becomes `NoBody`, for a building of no body alone.
- **Check:** tests of a circle's placement by hand-computed points: clear, touching a wall and a body at its edge and a hair inside it, past the map's bound, a hidden walker moved out; a builder's walk to its nearest point; the client preview's fit; the import's build of a cylinder fixture and its drop of a small one; `cargo drive run zero-hour`, on request, moves its counts, and its defense play gains an America case where a Patriot battery, ordered nothing, kills the bot's dozer.

### 2. A requirement of any one (S)

`production`, `import`, 04-capabilities/production.md, 09. After batch 1, independent of it; the Tomahawk needs the Strategy Center it lets stand.

- **The requirement.** An entry of `RequiresData::units` (`production/src/requires_data.rs:11`) is a name or a list of names; `RequirementBook` (`production/src/requirement_book.rs`, built at `production/src/book_builders.rs:309`) holds each entry as the types any one of which passes, and the start step's test of a train and a build passes an entry when the player holds one of them. Builds: a name is resolved once.
- **The import.** `ProductionImport::requires` (`apps/import/src/zero_hour/production_import/mod.rs:293`) converts a line of several objects to one entry; `ButtonDropCause::EitherOr` goes.
- **Check:** tests of a train that needs one of two types: refused with neither, passed with either, its holding counted as the types come and go; the import of a fixture's line of two objects; the Zero Hour check's count of builds.

### 3. Spared kin and a clip of one shot (S)

`combat`, `import`, 04-capabilities/combat.md, 09. After batch 2, which makes the Tomahawk trainable.

- **Spared kin.** `SplashData` (`combat/src/attack_data.rs`) and the run's `Splash` (`combat/src/weapon/mod.rs`) gain `spares_kin`; `Attacks::take_hits` (`combat/src/attacks.rs:134`) skips each ally of the source of the source's unit type. Builds: a capability serves every game.
- **The import.** `ImportedWeapon::relations` (`apps/import/src/zero_hour/combat_import/mod.rs:568`) converts `NOT_SIMILAR` to `spares_kin`, and `NOT_AIRBORNE` keeps `WeaponDrop::Affects`; a weapon of `ClipSize = 1` (`combat_import/mod.rs:381`, `:404`) converts to a period of its reload and no clip, so `WeaponDrop::NoDelay` stays for a weapon of more shots alone.
- **Check:** tests of a splash that spares an ally of its source's type and wounds an enemy of that type and an ally of another; the import of a Tomahawk fixture's splash and of a Stinger fixture's period; the Zero Hour check's weapons' counts.

### 4. Kept spawns (M)

`production`, `import`, the Zero Hour check, 04-capabilities/production.md, 09. After batch 3, as the Stinger soldiers fire the weapon it converts.

- **The spawns.** `ProductionData` (`production/src/production_data.rs:27`) and `ProducerSpec` (`production/src/producer_spec.rs:17`) gain `spawns = { unit_type, count, replace_ms }`; as a site completes (`production/src/construction/mod.rs:386`), on the server, its building sends `count` units out of its exit as `Trains::free_units` (`production/src/trains.rs:257`) does, keeps each spawn's tick of return as state of the match when it dies, sends it again at that tick, and kills its spawns as it dies. Builds: a kind is a marker, and an entity spawns whole; a pass visits what changed.
- **The import.** A `SpawnBehavior` of no `OneShot` (`apps/import/src/zero_hour/format/ini/object_ini/ini_supply.rs:77`) converts beside the free units (`apps/import/src/zero_hour/supply_import.rs:138`) to the producer's `spawns`; the tunnel defenders' `NoExit` goes.
- **Check:** tests of spawns: the count as the site completes, a spawn back at its hand-computed tick and not a tick before, the spawns' death with their building, and a snapshot that keeps the ticks of return; the import of a fixture's kept spawns; `cargo drive run zero-hour`, on request, pins a play where a GLA player's Stinger site's soldiers kill the bot's dozer.

### 5. A train's variants (S)

`production`, `import`, 04-capabilities/production.md, 09. After batch 4, independent of it; last of the engine's changes.

- **The variants.** `TrainData` (`production/src/train_data.rs:18`) gains `variants`, unit types of its package that stand in for its own; `Trains::finish_trains` (`production/src/trains.rs:199`) draws one for each unit from the producer's random stream, and the supply and the holdings count the train's type until it spawns. Builds: a name is resolved once.
- **The import.** A train of a type of `BuildVariations` (`apps/import/src/zero_hour/production_import/mod.rs:204`) makes its variations, each typed from its own object with its own look; the Technical's `NoWalker` goes.
- **Check:** tests of a train of three variants: a drawn type each time from a pinned seed, the same draw on a replay, and the cost and supply of the train's own type; the import of a fixture's variations; the Zero Hour check's count of trains.

### 6. The fold and the play (S)

04-capabilities/production.md, combat.md, 09. Last: it ends the slice.

- **The fold.** The decisions go to production.md, combat.md and 09, with 09's losses: the Stinger soldiers stand by the site's exit, not on its roof, and its damage does not pass to them; the tunnel holds no units; the facts stay in research/zero-hour-base-defenses.md, and the proposal is deleted.
- **Check:** the user plays Tournament Desert in the client and sees a Patriot battery, a tunnel network and a Stinger site fire at the bot's units, a killed Stinger soldier come back, Technicals of each chassis, an Inferno cannon and a Tomahawk fire.

## Foundations
