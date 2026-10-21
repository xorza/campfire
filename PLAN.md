# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Base defenses

A player builds a Patriot battery, a tunnel network and a Stinger site and sees each fire at an enemy that comes near, the site's three Stinger soldiers with missiles, each soldier back 30 s after it dies and the tunnel's two defenders 10 s after; and trains Technicals, each of one of three chassis, and Inferno cannons, and sees them fire, and Tomahawks after a Strategy Center ([proposal](docs/design/12-base-defenses.md)). The Scud launcher, which needs a science, the demo trap, which has no body, the weapons of `NOT_AIRBORNE`, and the Tomahawk's weapon, which waits a `PreAttackDelay`, stay dropped. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 4. Kept spawns (M)

`production`, `import`, the Zero Hour check, 04-capabilities/production.md, 09. First of what is left: the Stinger soldiers now fire their weapon.

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
