# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Defenses and airfields

The slice of [Defenses and airfields](../docs/design/defenses-and-airfields.md): a player of a Zero Hour skirmish kills an enemy Fire Base, Stinger Site and Battle Bus, trains no more jets than an airfield's places, sees doors open as units come out, and sees jets take off and land as the release game's. Its facts are [Zero Hour's bodies](../docs/research/zero-hour-bodies.md) and [Zero Hour's jets](../docs/research/zero-hour-jets.md); its checks are that document's.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert kills an enemy Fire Base and a Battle Bus, sees an airfield refuse a fifth jet, sees the doors open as units come out, and sees jets take off and land as the release game's, in the client; then the slice's decisions fold into `04-capabilities/combat.md`, `04-capabilities/production.md`, `04-capabilities/airfields.md`, `02-engine-core.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `defenses-and-airfields.md` goes.

## Exits, docks and selection

The slice of [Exits, docks and selection](../docs/design/exits-docks-and-selection.md): a player of a Zero Hour skirmish selects as in the release game, sees units drive out of their factories and trucks dock at their docks' points, puts soldiers in a battle bunker, hears every weapon, and plays with no crash. Its facts are [Zero Hour's exits, docks, bunkers and selection](../docs/research/zero-hour-exits-and-selection.md); its checks are that document's. It starts after Defenses and airfields' end.

### EX1. The use order's crash

Touches interaction and the client; it stands first, as the crash ends a match.

- [ ] An order to use or to enter panics the client in `UseOrders::ordered` (`use_orders.rs:51`) when the unit `Using` finds a use for holds no `StableId`, `SlotTimes` or `ActionProgress` beside its `ActionSlots`: the step first replays the crashed session's journal through a client to find the unit and the part it lacks, and on which world, then makes that part come with the unit where its owner declares it, or the two queries read one set of parts, so the order cannot find a use the slots query refuses. Checks: a test of the case the replay finds, and the client plays the replayed match past its tick. Builds: What can be forgotten is a type.

### EX2. A gated weapon's look

Touches the Zero Hour import; it stands beside EX1, as it changes no system.

- [ ] A weapon of a set an upgrade chooses plays no fire sound and no fire effect (exits-docks-and-selection.md, A look follows its converted action): `EffectsImport::action_looks` (`effects_import/mod.rs:330`) looks each weapon up by `weapon_<name>`, while the combat import names it `_starting` or `_upgraded` (`combat_import/weapon_gate`), so 39 actions, the Inferno Cannon's among them, have no look. Checks: the import's test gives a gated weapon its look under each converted action; the Zero Hour check's look counts move, each named. Builds: A name is resolved once.

### EX3. The minimum range in play

Touches combat and orders, only if the check finds a defect.

- [ ] The Inferno Cannon was seen to fire at a target too near, while the import gives its weapon `min_range = 50` and the engine refuses an attack within it and walks its attacker away (`targets.rs:102`, `destinations.rs:125`): the step adds a case to the Zero Hour check that orders an Inferno Cannon to attack a unit 30 m off, and lets an enemy walk within 50 m of an idle one, and pins that it fires at neither until it stands past 50 m; a defect it finds is fixed where the reach is tested. Checks: that case; the Zero Hour check's goldens do not move otherwise. Builds: A capability serves every game.

### EX4. Selection

Touches the client; it stands apart from the sim's steps.

- [ ] A drag selects buildings, no key adds to a selection, and a double click is two clicks (exits-docks-and-selection.md, A drag selects no building; exits-docks-and-selection.md, Shift adds, a double click takes a kind): `Selecting::release` (`selecting.rs:164`) selects every own unit in a drag's rectangle, structures among them, and a click selects one alone. The input map gains the add key and the map key; a drag skips a unit of the engine tag of a structure; the add key adds a click's and a drag's units and removes a click's selected ones; a double click within the system's interval selects every own unit of the clicked one's type the screen shows, or the map holds with the map key. Checks: the client's tests of the design's Checks. Builds: One word, one meaning.

### EX5. Exits

Touches production and the Zero Hour import.

- [ ] A trained unit appears in the open cell nearest its producer's exit point, on any side of the body (exits-docks-and-selection.md, A trained unit drives out of its producer): `TrainPoints::spawn_point` (`trains.rs:696`) searches the nearest open cell. A unit is made at the exit point, dashed through its producer's body to the natural rally point, then routed to the rally point; a point outside every body keeps today's spawn. Checks: the production tests of the design's Checks; the Zero Hour check's production and fight plays move by each exit's drive, each named, and its bots' match still ends. Builds: A capability serves every game.

### EX6. Docks

Touches production and the Zero Hour import.

- [ ] A gatherer docks 10 m from any side of a dock or a center, and waits at its side (exits-docks-and-selection.md, A dock has its points; 09-zero-hour.md, A gatherer docks as a dozer builds; 09-zero-hour.md, Supply): the gather loop's reach of the box (`gather_loop`) and the import's `DOCK_RANGE` (`supply_import.rs:104`). A node's and a drop-off's `dock` gives the start, action, end and waiting points, which the gather loop drives through, and the import reads `DockStart`, `DockAction`, `DockEnd` and `DockWaiting<n>` from the dock's and the center's models. Checks: the production tests of the design's Checks; the Zero Hour check's supply play moves by each dock's drive, each named, and its bots' match still ends. Builds: A capability serves every game.

### EX7. Bunkers

Touches interaction and the Zero Hour import.

- [ ] No unit enters an Overlord or a Helix, upgraded or not (exits-docks-and-selection.md, A bunker is an entry a tag opens; 09-zero-hour.md, Helicopters; 09-zero-hour.md, Transports and garrisons): the import gives neither an `enter`. A container's `enter` gains `when`, the tags that open it and let its riders fire; the import gives the Overlord an entry of 5 infantry opened by its battle bunker's tag, and the Helix one of 5 infantry and vehicles, its riders firing with its bunker's tag. Checks: the interaction and import tests of the design's Checks; the Zero Hour check plays a soldier into an Overlord's bunker; its bots' match still ends. Builds: A capability serves every game.

### EX8. A sold container

Touches interaction and production; it stands after EX7, which changes containers.

- [ ] A container that is sold leaves its riders out of the world (exits-docks-and-selection.md, A sold container lets its riders out): `Sales::end_sales` despawns it (`sales.rs:127`), and `Containers::fall` lets riders out only on `Added<Dead>` (`containers.rs:225`). A sale's start orders the container's riders out, through a contract below production and interaction, and a container's despawn despawns the riders still inside. Checks: the interaction tests of the design's Checks. Builds: Each crate depends only on lower layers. Takes the issue of a container that leaves by a despawn.

### EX9. A Troop Crawler's riders

Touches control or interaction, as the issue's decision says; it waits for that decision.

- [ ] A bot never lets out the riders its containers come with, so a Troop Crawler's 8 Red Guards never fight (Decide: Control): Zero Hour's crawler is an assault transport whose attack order lets out each healthy rider to fight and calls back each hurt one (`AssaultTransportAIUpdate`), which no capability holds. The step does what the decision picks. Checks: the Zero Hour check's base match plays the crawler's riders out, its end moves, named, and still ends. Builds: A capability serves every game.

### EX10. A base start's room

Touches navigation and the mode; it stands apart from the sim's other steps.

- [ ] America's first Ranger of a base start on Tournament Desert stands where its base's buildings enclose it (exits-docks-and-selection.md, A base start's room connects): `NavigationColumn::room_near` (`navigation_column.rs:177`) takes any cell clear of bodies. A room is a cell of the walkable region the map's ground mostly shares, with the bodies the call has placed as walls. Checks: the mode's test of the design's Checks; the Zero Hour check's base start places no unit enclosed, and its goldens move, named. Builds: A capability serves every game. Takes the issue of the enclosed Ranger.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert as China against the America builder bot does what the design's last check says, in the client; then the slice's decisions fold into `02-engine-core.md`, `04-capabilities/production.md`, `04-capabilities/interaction.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `exits-docks-and-selection.md` goes.

## Changed

- **DA4. A jet's takeoff and approach.** The sortie's own roll, not a navigation mover, with a flight goal of a height and a climb; the jet flies home at its type's height and glides from the height it holds at the approach, not from the approach's height, as only the landing state of Zero Hour descends to it; the Zero Hour check pins the takeoff's ticks, 142, not 110, and the landing's, and the airfields test pins the heights; the bots' match does not move.
- **DA3. Doors and hangars.** Every airfield makes its jet in its place's hangar, as `exitObjectViaDoor` does, so the hangar went to America's places as a place's `exit`, from which the jet taxis to its place, and China's, parked in their hangars, kept theirs; a place's door is held open while its place holds a unit, as `setHoldDoorOpen` does, which the slice had left; the queue's due waits for the door its head's unit waits for; the check's dozer, crusaders, Paladin and Raptor wait their doors' openings, the Raptor taxis 19 ticks out of its hangar, the door flags make 828 more look states, and the base match ends at tick 10,639, not 15,460; a train of no place at a producer of places, as a helipad's, takes no door.
- **DA2. A place reserved as the train queues.** `HeldPlaces::admits` holds the rule, which the train's admission, the bots' start check and the panel's dimming each read; the wait at the queue's end stays for a place an airfield gives a jet of a gone one; no play of the Zero Hour check queues a fifth jet, as no play orders more jets than its airfield's places, and its goldens do not move.
- **DA1. Hive and undead bodies.** The second life is combat's `second_life = { modifier, lethal }`, whose modifier raises the life stat and gives the tag `second_life`, which holds the unit where it stands and tests its second armor, and whose `lethal` damage kinds, `unresistable` for the import, kill through it; the unit's life fills to its first maximum as the second life starts, and the tick's stats raise it and its maximum by the modifier's raise, so a second damage in that tick takes the first maximum, a loss the design records; the check's `unliving` falls by 15, to 20, the 4 Battle Buses of the types live twice, and the base match ends at tick 15,460; no play kills a Fire Base or a bus, as no play's start holds an enemy one in reach, and the combat tests hold the second life.

## Decisions

- **The takeoff is the sortie's.** The sortie rolls and lifts its jet each tick through `FlightGoal::Toward`, as Zero Hour's `JetTakeoffOrLandingState` drives its locomotor along a path with a lift it sets each frame. Refused: a winged mover in navigation's `zero_hour`, which would carry `Locomotor::moveTowardsPositionWings`'s forces, mass and damping for a path the sortie already knows; source: GeneralsX `JetAIUpdate.cpp`, `Locomotor.cpp`.
- **A place's door is held while its place holds a unit.** Zero Hour holds an airfield's door open while a jet holds its place (`ParkingPlaceBehavior::reserveSpace`, `ProductionUpdate::setHoldDoorOpen`), which the slice had left as a loss; the door state holds it, and the client times a door only while its stage changes. Refused: the loss, under which a hangar closes on its jet; source: GeneralsX `ParkingPlaceBehavior.cpp`.

## Foundations
