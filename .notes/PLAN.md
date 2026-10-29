# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Entries and blocking

The slice of [Entries and blocking](../docs/design/entries-and-blocking.md): a player of a Zero Hour skirmish puts a group of soldiers into a standing or a driving container and sees each walk straight in, the container keeping its own course, and sends columns across each other and through gaps and sees each unit keep its line and wait behind the unit ahead. It stands before the end of Riders and upgrades, whose player's check needs riders that enter. Its facts are [Crowd movement](../docs/research/crowd-movement.md) and [Zero Hour's portable structures and entries](../docs/research/zero-hour-portable-structures.md); its checks are the design's.

### Batch 2: blocking

`navigation`'s walking, steering and collision, and the movement models of `navigation::zero_hour`: the crowd model the user chose on 29 October 2026, Zero Hour's brake in place of the push.

- [ ] **EB4. A walker brakes behind a unit that blocks it.** `Walking::move_units` (`navigation/src/walking.rs:451`) steps each walker its whole step and leaves it to collision; a walker's step and a mover's speed are cut at the unit in its cone, by that unit's advance of the tick before, a part of each walker, saved and predicted. Builds: what can be forgotten is a type, as the advance is state. Check: the navigation tests of the cone's bounds, of a slower walker ahead, of three abreast and of two columns crossing.
- [ ] **EB5. A unit outside the cone neither blocks nor pushes.** `Collider::push` (`navigation/src/collider/mod.rs:96`) and personal space (`:84`) go, and `Collider::separate` (`:404`) moves the walker whose cone holds the other; `Steering::crowded` measures by the bodies touching, and the broadphase reaches the bodies alone (`navigation/src/broadphase.rs:75, 130`). Check: the navigation tests of a gap, of a corridor and of a walker past a unit outside its cone; the goldens of the crowd cases move, each move stated.
- [ ] **EB6. A head-on block steps the lower priority aside.** Only an idle friend steps aside (`Steering::asides`), and a stuck walker steers round the walkers it touches, shifted to its left (`navigation/src/steering/mod.rs:434-444`); of two walkers that block each other head on, the lower priority, by weight, then the one behind, then the higher stable id, steps aside to its right by a detour at the head of its route and walks on, and the shift goes. Builds: a module's rule lives once, as the step aside serves an idle friend and a walker alike. Check: the navigation tests of two walkers head on in the open, of a soldier and a vehicle, of a lane with an alcove and with none, and of a walker blocked by an enemy that stands.

### Batch 3: the Zero Hour check

The import's plays, measured, with the slice's whole change.

- [ ] **EB7. The Zero Hour check enters and passes.** The check plays China's infantry into a Humvee, a Helix and an Overlord's bunker, each entering up to its places with the container's place unchanged; its bases and attack plays end, and their ticks and stalls, run before EB4 and after EB6, are written side by side into `docs/research/crowd-movement.md`. Check: `cargo drive run zero-hour`, its golden moved with each move stated.

### End of the slice

- [ ] A player of Tournament Desert as China against the America builder bot does what the design's last check says, in the client; then the slice's decisions fold into `04-capabilities/navigation.md`, `04-capabilities/interaction.md` and `09-zero-hour.md`, the decisions they replace there go, and `entries-and-blocking.md` goes.

## Riders and upgrades

The slice of [Riders and upgrades](../docs/design/riders-and-upgrades.md): a player of a Zero Hour skirmish puts any soldier into a Helix and an Overlord's bunker, sees riders follow a transport that drives, buys one of the Overlord's and the Helix's three upgrades and no second, and sees the Gattling cannon's guns fire from its carrier and the Helix's riders fire from its bunker. Its facts are [Zero Hour's portable structures and entries](../docs/research/zero-hour-portable-structures.md); its checks are that document's.


### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert as China against the America builder bot does what the design's last check says, in the client; then the slice's decisions fold into `04-capabilities/actions.md`, `04-capabilities/interaction.md`, `04-capabilities/production.md`, `04-capabilities/navigation.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `riders-and-upgrades.md` goes.

## Changed

- **EB1. A walk may go into a unit.** `Destination` holds a `WalkGoal`, a point or a unit with its place, `WalkItem::to_unit` sets one, a route asks for one, and an approach of the `enter` kind walks into its target.

- **EB2. A walker passes through the unit it walks into.** Steering's steps aside, short routes, crowded goals and packed crowds, the crowd's price and walls of a long route and of its straight line, and collision skip the unit the walker's goal names, both ways in collision; three riders ordered at once into an idle humvee that drives all enter, the humvee standing where it stood, where the third gave up 1.55 m from it before.

- **EB3. A walk into a unit follows it.** A route serves a walk into a unit while the unit stands within a tenth of the walker's distance of the route's goal, and a new route keeps the walker's progress; a walker's progress grows by each move of its goal, a chase's too, so a rider that keeps pace with a humvee is never stuck, where it was stuck in 5 ticks. That rule of progress moved the proving match's behaviour from tick 195 and the 3v3's from tick 2,583, their goldens written again, the proving match's projectiles in flight summing 245 in place of 239; the new layouts of `Route` and `Destination` raised the snapshot's data version to 99, with the layout golden, the verifier's fixture and the sim's state digest written again, and a spectator's join burst grew 24 bytes, to 5,200.

- **RU2. An entry walks into its container.** The Helix's riders stopped short not at its body, which is on the air layer, but at the building its base start placed it over, then behind the base's vehicles; so a flier that lands for its riders takes the new goal `land`, under which navigation flies it first to the nearest point clear of the static bodies and the ground units, as the Chinook's landing finds its point, and the scripted Helix took its 5 places of 8 soldiers, the other 3 refused as it was full; a rider already followed a container that drives, which the interaction test now holds; no play of the Zero Hour check moves.

- **RU5. A portable structure is a part of its carrier.** The import gives every portable structure an upgrade makes inside its carrier its creation tag, `<trigger>_created`, so the Overlord's and the Helix's three upgrades now each hold back the other two, and folds the Gattling cannon's two weapons into the Overlord, the Helix and their generals' copies, in the slots past the carrier's own, named for the structure and armed by the tag; the Overlord keeps the upgrade's anti-air dummy of 0.0001 damage, harmless beside the cannon's guns. The structures' models, drawn at the carrier's bone, and the propaganda tower's heal, an aura, came later from the issue log; the check counts 4 more weapons, fire effects and fire sounds, and 22 more upgrade modules, and no play moves.

- **RU6. A rider's fire follows its container's tags.** A container's `fire_when` arms its riders from the gap after Collide's stats, inserting or taking each rider's `FiringRider` as its tags change, and the sim's new event `PassesResumed` pushes an armed rider's timers again, as the passes dropped them while it was out of the world and not firing; the import gives a transport whose `PassengersFireUpgrade` it holds its infantry's fire under the upgrade's tag, its first trigger suffixed `fire`, so the Helix's riders fire once its bunker is bought; the check counts 5 more upgrade modules, and no play moves.

## Declined

- **RU3. The client's order into an opened bunker.** A net test showed a predicting client holding an object research's modifier and its upgrade's tag as the server does, and the player's log holds no order of any kind at the bunkered Overlord, so no defect of the client was found; both Overlords looked alike, with no bunker drawn, which RU5 draws.

## Decisions

- **A conflict holds back both of its sides.** A research is refused, and its button dimmed, while its holder holds or queues a modifier whose upgrade conflicts with its own, read from the type's upgrades by `TypeUpgrades::withheld`, or while either holder, its player or its unit, holds or queues a modifier that conflicts with an upgrade its own triggers, by `TypeUpgrades::withholding`, as the upgrade's own test reads both, so the propaganda tower, which the import gives no upgrade, is held back by the bunker's and the cannon's; the panel reads its unit's own queue alone. Refused: the conflicts of an upgrade the research triggers alone, under which the tower's research paid while a bunker withheld nothing of it; a command set swapped at each upgrade, as Zero Hour's `CommandSetUpgrade` does, which the panel would hold for one game; source: GeneralsX `UpgradeModule.cpp`, `CommandSet.ini`.

- **A flier lands where it has room.** A flier landing for its riders flies to the center of the nearest cell a walker of its radius stands in on the first layer, clear of the units' bodies there, and lands there, as Zero Hour's `ChinookTakeoffOrLandingState` lands at the point `PartitionManager::findPositionAround`, which takes no point inside an object by default (`FindPositionOptions`), and `Pathfinder::adjustToLandingDestination` find. Refused: Zero Hour's route that crosses the airfield a container is parked on (`Pathfinder::findPath`, `KINDOF_FS_AIRFIELD`), which needs a route and a collision that ignore one static body, for a landing the original makes anyway; a landing clear of the static bodies alone, under which the Helix landed among the base's vehicles and 3 soldiers stood behind them; source: GeneralsX `ChinookAIUpdate.cpp`, `AIPathfind.cpp`.
