# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Defenses and airfields

The slice of [Defenses and airfields](../docs/design/defenses-and-airfields.md): a player of a Zero Hour skirmish kills an enemy Fire Base, Stinger Site and Battle Bus, trains no more jets than an airfield's places, sees doors open as units come out, and sees jets take off and land as the release game's. Its facts are [Zero Hour's bodies](../docs/research/zero-hour-bodies.md) and [Zero Hour's jets](../docs/research/zero-hour-jets.md); its checks are that document's.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert kills an enemy Fire Base and a Battle Bus, sees an airfield refuse a fifth jet, sees the doors open as units come out, and sees jets take off and land as the release game's, in the client; then the slice's decisions fold into `04-capabilities/combat.md`, `04-capabilities/production.md`, `04-capabilities/airfields.md`, `02-engine-core.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `defenses-and-airfields.md` goes.

## Exits, docks and selection

The slice of [Exits, docks and selection](../docs/design/exits-docks-and-selection.md): a player of a Zero Hour skirmish selects as in the release game, sees units drive out of their factories and trucks dock at their docks' points, puts soldiers in a battle bunker, hears every weapon, and plays with no crash. Its facts are [Zero Hour's exits, docks, bunkers and selection](../docs/research/zero-hour-exits-and-selection.md); its checks are that document's. It starts after Defenses and airfields' end.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert as China against the America builder bot does what the design's last check says, in the client; then the slice's decisions fold into `02-engine-core.md`, `04-capabilities/production.md`, `04-capabilities/interaction.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `exits-docks-and-selection.md` goes.

## Changed

- **EX10. A base start's room.** A room connects when a flood of the search's points that have room, along the grid's axes, reaches the search's edge, not the walkable region of the map's ground, which the pathing grid knows without the bodies the call spawned; with none connected, any room serves, as a search inside an enclosure would find none; every play that starts with `room_near` moves: the transport loads at tick 989, not 1,312, the garrison outranges at 1,351, not 1,711, the combat's rockets at 3, not 6, and the base match ends at tick 11,408, not 9,132.
- **EX9. A Troop Crawler's riders.** The decision taken was the assault transport, a container's `assault`, which an attack or an attack-move order answers by letting out each rider above its share of life; the bots' army takes a unit that assaults by its riders, so the crawlers of a China base attack-move with the army, and the base match ends at tick 9,132, not 9,135; an unarmed crawler takes no attack order of one target, as the order checks its own weapons, and its call of hurt riders back stays a loss.
- **EX7. Bunkers.** An entry's `open` holds the tags that open it, not the fire, as a rider's fire is fixed as it enters, so the Helix's riders do not fire from its bunker, a loss; the Overlord's bunker is the transport its upgrade's list makes inside it, and its upgrade gives the tag; no play of the check enters a bunker, as the interaction and import tests hold the rule, and the check's transports rise by 9 to 55.
- **EX6. Docks.** A node and a drop-off name one dock point, where a gatherer docks within its gather's reach, not the start, end and waiting points, whose drive and queue would make the loop hold a path and a queue of places, a loss for now; only America's supply center and the supply warehouse give the bone, so the check's helicopter play moves, its Chinook delivering at tick 618, not 605, and the others dock at their body as before.
- **EX4. Selection.** A drag takes the player's units that walk, as the engine has no tag of a structure and a unit that does not walk is its building; the keys file gains the add key, Shift, the map key, Alt, and the press's double click time, Windows' 0.5 s, which the import keeps, as Zero Hour takes the system's.
- **EX5. Exits.** A unit made inside its producer leaves by a new forced move, an exit, which crosses its producer's body alone, as a dash stops at every static body; the check's War Factory and barracks units drive out, the fight's first shot comes at tick 905, not 871, the supply's first delivery at 491, not 402, the missile play's shot at 1,088, not 1,146, the transport's fall at 128, not 98, and the base match ends at tick 9,135, not 10,639.
- **EX1. The use order's crash.** The crashed session's journal was gone with the session's scratch directory, so no replay found the unit; the use query reads only units that hold their slots' times and progress beside their id, the parts the order writes, so an order cannot find a use the slots query refuses, and a test of a unit with no slot times holds it; which unit a client held without them stays unknown, the likeliest one whose times, which only its owner receives, had not come.
- **EX2. A gated weapon's look.** The combat import gives each game weapon's converted actions, and the effects import keys the weapon's look by each; the check counts 20 more fire effects, 8 more hit ends and 34 more fire sounds, and no play moves.
- **DA4. A jet's takeoff and approach.** The sortie's own roll, not a navigation mover, with a flight goal of a height and a climb; the jet flies home at its type's height and glides from the height it holds at the approach, not from the approach's height, as only the landing state of Zero Hour descends to it; the Zero Hour check pins the takeoff's ticks, 142, not 110, and the landing's, and the airfields test pins the heights; the bots' match does not move.
- **DA3. Doors and hangars.** Every airfield makes its jet in its place's hangar, as `exitObjectViaDoor` does, so the hangar went to America's places as a place's `exit`, from which the jet taxis to its place, and China's, parked in their hangars, kept theirs; a place's door is held open while its place holds a unit, as `setHoldDoorOpen` does, which the slice had left; the queue's due waits for the door its head's unit waits for; the check's dozer, crusaders, Paladin and Raptor wait their doors' openings, the Raptor taxis 19 ticks out of its hangar, the door flags make 828 more look states, and the base match ends at tick 10,639, not 15,460; a train of no place at a producer of places, as a helipad's, takes no door.
- **DA2. A place reserved as the train queues.** `HeldPlaces::admits` holds the rule, which the train's admission, the bots' start check and the panel's dimming each read; the wait at the queue's end stays for a place an airfield gives a jet of a gone one; no play of the Zero Hour check queues a fifth jet, as no play orders more jets than its airfield's places, and its goldens do not move.
- **DA1. Hive and undead bodies.** The second life is combat's `second_life = { modifier, lethal }`, whose modifier raises the life stat and gives the tag `second_life`, which holds the unit where it stands and tests its second armor, and whose `lethal` damage kinds, `unresistable` for the import, kill through it; the unit's life fills to its first maximum as the second life starts, and the tick's stats raise it and its maximum by the modifier's raise, so a second damage in that tick takes the first maximum, a loss the design records; the check's `unliving` falls by 15, to 20, the 4 Battle Buses of the types live twice, and the base match ends at tick 15,460; no play kills a Fire Base or a bus, as no play's start holds an enemy one in reach, and the combat tests hold the second life.

## Declined

- **EX3. The minimum range in play.** Every reach test of an attack, a scan and a chase holds `min_range` as Zero Hour measures it, edge to edge (`Weapon::isWithinAttackRange`, `FROM_BOUNDINGSPHERE_2D`), and the order and scan tests pin it; a play of the check would find no defect, so the issue waits for a report that replays one.

## Decisions

- **The takeoff is the sortie's.** The sortie rolls and lifts its jet each tick through `FlightGoal::Toward`, as Zero Hour's `JetTakeoffOrLandingState` drives its locomotor along a path with a lift it sets each frame. Refused: a winged mover in navigation's `zero_hour`, which would carry `Locomotor::moveTowardsPositionWings`'s forces, mass and damping for a path the sortie already knows; source: GeneralsX `JetAIUpdate.cpp`, `Locomotor.cpp`.
- **A place's door is held while its place holds a unit.** Zero Hour holds an airfield's door open while a jet holds its place (`ParkingPlaceBehavior::reserveSpace`, `ProductionUpdate::setHoldDoorOpen`), which the slice had left as a loss; the door state holds it, and the client times a door only while its stage changes. Refused: the loss, under which a hangar closes on its jet; source: GeneralsX `ParkingPlaceBehavior.cpp`.
