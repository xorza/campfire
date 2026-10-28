# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Riders and upgrades

The slice of [Riders and upgrades](../docs/design/riders-and-upgrades.md): a player of a Zero Hour skirmish puts any soldier into a Helix and an Overlord's bunker, sees riders follow a transport that drives, buys one of the Overlord's and the Helix's three upgrades and no second, and sees the Gattling cannon, the propaganda tower and the bunker stand on their carrier and work. Its facts are [Zero Hour's portable structures and entries](../docs/research/zero-hour-portable-structures.md); its checks are that document's.

### Batch 1: the walk to a container

It touches `abilities`, `interaction` and the client, and stands first, as every later step plays riders that must reach their container.

- [ ] **RU1. An approach that cannot reach ends.** `Casts::start_casts` (`abilities/src/casts.rs:211`) sets an approaching cast's walk each tick through `WalkItem::to` (`core/units/src/walk.rs:36`), which keeps a unit that arrived short of its goal standing with no destination, so its order stays approaching forever: 5 of 8 China soldiers ordered into a Helix stood 65 m to 145 m off with no walk to the end of the scripted match. The approach ends its order once its walk gives up, as an attack's chase does (Direction: what can be forgotten is a type). Check: an actions test of a cast at a target no path reaches ends its order and frees the unit for the next.
- [ ] **RU2. An entry walks into its container.** An entry's approach walks to its container's point and stops at its body, and the scripted Helix's riders arrived short; find why they stopped so far off, then make the entry's walk ignore its container's body and aim each tick at its point, as `AIEnterState` does, so a rider reaches a container that stands among bodies and follows one that drives, which the player reports riders do not. Check: interaction tests of a rider entering a container ringed by static bodies and one that drives away; the Zero Hour check's transport play boards all 8 soldiers into a Helix.
- [ ] **RU3. The client's order into an opened bunker.** The client's right click on an Overlord whose bunker opened sends no enter order: the replayed log of the player's match holds none, though the server's `Using::entry` (`interaction/src/using.rs:99`) took each of 17 soldiers. Find where a predicting client's world differs, its Overlord's tags or its riders' slots, by a test of `UseClickPlugin::use_unit` (`client/src/use_click.rs:27`) in a client's world of a predicted Overlord whose upgrade completed, and fix it there. Check: that test orders each selected soldier in.

### Batch 2: one upgrade of three

It touches `production`, the client's panel and the import, and stands before the structures, which each upgrade shows.

- [ ] **RU4. A conflicting upgrade is refused.** The Overlord's and the Helix's three researches stay in their slots after one completes, and a second pays for an upgrade its conflict then withholds (`trains.rs:224`, `admits`). A research is refused, and its button dimmed (`command_panel/mod.rs:799`), while its holder holds or researches a modifier the upgrade it triggers conflicts with, read from the type's upgrades. Check: a production test refuses the second research and refunds nothing it did not take; the import's upgrade test keeps the three conflicts.

### Batch 3: the structures on their carriers

It touches the import, `interaction`'s fire and `combat`'s weapons of a tag, and stands last, as it needs the upgrades of batch 2.

- [ ] **RU5. A portable structure is a part of its carrier.** The import drops the Overlord's and the Helix's Gattling cannon and propaganda tower, gives the Overlord the Gattling upgrade's dummy weapon of 0.0001 damage alone, and draws no structure (`zero_hour/portable_structure.rs:18`, `upgrade_import/mod.rs:113`). It folds each structure into its carrier: the cannon's `GattlingBuildingGun` and `GattlingBuildingGunAir` and its detection, and the tower's heal of 1 % a second within 150, held while the upgrade's tag holds; each structure's model merged into the carrier's models at its bone, shown by the tag as a `SubObjectsUpgrade`'s nodes are (`model_writer.rs:236`); the dummy weapon dropped. Check: the import's tests give both carriers the cannon's weapons and the tower's aura under their tags, and the merged nodes; the Zero Hour check pins the upgrades' weapons and looks.
- [ ] **RU6. A rider's fire follows its container's tags.** A rider's fire is fixed as it enters (`Riders::enter`, `FireSpec::fires`), so the Helix's riders do not fire from its bunker. A container's `fire` names `when`, tags its unit holds for its riders to fire, which the fire pass tests each tick, and the import gives the Helix's bunker upgrade's tag (`PassengersFireUpgrade`). Check: an interaction test of a rider that entered before its container's tag fires once the tag holds.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert as China against the America builder bot does what the design's last check says, in the client; then the slice's decisions fold into `04-capabilities/actions.md`, `04-capabilities/interaction.md`, `04-capabilities/production.md`, `02-engine-core.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `riders-and-upgrades.md` goes.
