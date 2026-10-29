# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Riders and upgrades

The slice of [Riders and upgrades](../docs/design/riders-and-upgrades.md): a player of a Zero Hour skirmish puts any soldier into a Helix and an Overlord's bunker, sees riders follow a transport that drives, buys one of the Overlord's and the Helix's three upgrades and no second, and sees the Gattling cannon's guns fire from its carrier and the Helix's riders fire from its bunker. Its facts are [Zero Hour's portable structures and entries](../docs/research/zero-hour-portable-structures.md); its checks are that document's.


### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert as China against the America builder bot does what the design's last check says, in the client; then the slice's decisions fold into `04-capabilities/actions.md`, `04-capabilities/interaction.md`, `04-capabilities/production.md`, `04-capabilities/navigation.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `riders-and-upgrades.md` goes.

## Changed

- **RU2. An entry walks into its container.** The Helix's riders stopped short not at its body, which is on the air layer, but at the building its base start placed it over, then behind the base's vehicles; so a flier that lands for its riders takes the new goal `land`, under which navigation flies it first to the nearest point clear of the static bodies and the ground units, as the Chinook's landing finds its point, and the scripted Helix took its 5 places of 8 soldiers, the other 3 refused as it was full; a rider already followed a container that drives, which the interaction test now holds; no play of the Zero Hour check moves.

- **RU5. A portable structure is a part of its carrier.** The import gives every portable structure an upgrade makes inside its carrier its creation tag, `<trigger>_created`, so the Overlord's and the Helix's three upgrades now each hold back the other two, and folds the Gattling cannon's two weapons into the Overlord, the Helix and their generals' copies, in the slots past the carrier's own, named for the structure and armed by the tag; the Overlord keeps the upgrade's anti-air dummy of 0.0001 damage, harmless beside the cannon's guns. The structures' models, which need a model drawn at another's bone, and the propaganda tower's heal, which needs `PropagandaTowerBehavior`, went to the issue log, as each is a general feature of its own; the check counts 4 more weapons, fire effects and fire sounds, and 22 more upgrade modules, and no play moves.

- **RU6. A rider's fire follows its container's tags.** A container's `fire_when` arms its riders from the gap after Collide's stats, inserting or taking each rider's `FiringRider` as its tags change, and the sim's new event `PassesResumed` pushes an armed rider's timers again, as the passes dropped them while it was out of the world and not firing; the import gives a transport whose `PassengersFireUpgrade` it holds its infantry's fire under the upgrade's tag, its first trigger suffixed `fire`, so the Helix's riders fire once its bunker is bought; the check counts 5 more upgrade modules, and no play moves.

## Declined

- **RU3. The client's order into an opened bunker.** A net test showed a predicting client holding an object research's modifier and its upgrade's tag as the server does, and the player's log holds no order of any kind at the bunkered Overlord, so no defect of the client was found; both Overlords looked alike, with no bunker drawn, which RU5 draws.

## Decisions

- **A conflict holds back both of its sides.** A research is refused, and its button dimmed, while its holder holds or queues a modifier that conflicts with an upgrade its own triggers, or whose upgrade conflicts with its own, read from the type's upgrades by `TypeUpgrades::conflicting`, so the propaganda tower, which the import gives no upgrade, is held back by the bunker's and the cannon's; the panel reads its unit's own queue alone. Refused: the conflicts of an upgrade the research triggers alone, under which the tower's research paid while a bunker withheld nothing of it; a command set swapped at each upgrade, as Zero Hour's `CommandSetUpgrade` does, which the panel would hold for one game; source: GeneralsX `UpgradeModule.cpp`, `CommandSet.ini`.

- **A flier lands where it has room.** A flier landing for its riders flies to the center of the nearest cell a walker of its radius stands in on the first layer, clear of the units' bodies there, and lands there, as Zero Hour's `ChinookTakeoffOrLandingState` lands at the point `PartitionManager::findPositionAround`, which takes no point inside an object by default (`FindPositionOptions`), and `Pathfinder::adjustToLandingDestination` find. Refused: Zero Hour's route that crosses the airfield a container is parked on (`Pathfinder::findPath`, `KINDOF_FS_AIRFIELD`), which needs a route and a collision that ignore one static body, for a landing the original makes anyway; a landing clear of the static bodies alone, under which the Helix landed among the base's vehicles and 3 soldiers stood behind them; source: GeneralsX `ChinookAIUpdate.cpp`, `AIPathfind.cpp`.
