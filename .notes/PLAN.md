# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Defenses and airfields

The slice of [Defenses and airfields](../docs/design/defenses-and-airfields.md): a player of a Zero Hour skirmish kills an enemy Fire Base, Stinger Site and Battle Bus, trains no more jets than an airfield's places, sees doors open as units come out, and sees jets take off and land as the release game's. Its facts are [Zero Hour's bodies](../docs/research/zero-hour-bodies.md) and [Zero Hour's jets](../docs/research/zero-hour-jets.md); its checks are that document's.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert kills an enemy Fire Base and a Battle Bus, sees an airfield refuse a fifth jet, sees the doors open as units come out, and sees jets take off and land as the release game's, in the client; then the slice's decisions fold into `04-capabilities/combat.md`, `04-capabilities/production.md`, `04-capabilities/airfields.md`, `02-engine-core.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `defenses-and-airfields.md` goes.

## Changed

- **DA4. A jet's takeoff and approach.** The sortie's own roll, not a navigation mover, with a flight goal of a height and a climb; the jet flies home at its type's height and glides from the height it holds at the approach, not from the approach's height, as only the landing state of Zero Hour descends to it; the Zero Hour check pins the takeoff's ticks, 142, not 110, and the landing's, and the airfields test pins the heights; the bots' match does not move.
- **DA3. Doors and hangars.** Every airfield makes its jet in its place's hangar, as `exitObjectViaDoor` does, so the hangar went to America's places as a place's `exit`, from which the jet taxis to its place, and China's, parked in their hangars, kept theirs; a place's door is held open while its place holds a unit, as `setHoldDoorOpen` does, which the slice had left; the queue's due waits for the door its head's unit waits for; the check's dozer, crusaders, Paladin and Raptor wait their doors' openings, the Raptor taxis 19 ticks out of its hangar, the door flags make 828 more look states, and the base match ends at tick 10,639, not 15,460; a train of no place at a producer of places, as a helipad's, takes no door.
- **DA2. A place reserved as the train queues.** `HeldPlaces::admits` holds the rule, which the train's admission, the bots' start check and the panel's dimming each read; the wait at the queue's end stays for a place an airfield gives a jet of a gone one; no play of the Zero Hour check queues a fifth jet, as no play orders more jets than its airfield's places, and its goldens do not move.
- **DA1. Hive and undead bodies.** The second life is combat's `second_life = { modifier, lethal }`, whose modifier raises the life stat and gives the tag `second_life`, which holds the unit where it stands and tests its second armor, and whose `lethal` damage kinds, `unresistable` for the import, kill through it; the unit's life fills to its first maximum as the second life starts, and the tick's stats raise it and its maximum by the modifier's raise, so a second damage in that tick takes the first maximum, a loss the design records; the check's `unliving` falls by 15, to 20, the 4 Battle Buses of the types live twice, and the base match ends at tick 15,460; no play kills a Fire Base or a bus, as no play's start holds an enemy one in reach, and the combat tests hold the second life.

## Decisions

- **The takeoff is the sortie's.** The sortie rolls and lifts its jet each tick through `FlightGoal::Toward`, as Zero Hour's `JetTakeoffOrLandingState` drives its locomotor along a path with a lift it sets each frame. Refused: a winged mover in navigation's `zero_hour`, which would carry `Locomotor::moveTowardsPositionWings`'s forces, mass and damping for a path the sortie already knows; source: GeneralsX `JetAIUpdate.cpp`, `Locomotor.cpp`.
- **A place's door is held while its place holds a unit.** Zero Hour holds an airfield's door open while a jet holds its place (`ParkingPlaceBehavior::reserveSpace`, `ProductionUpdate::setHoldDoorOpen`), which the slice had left as a loss; the door state holds it, and the client times a door only while its stage changes. Refused: the loss, under which a hangar closes on its jet; source: GeneralsX `ParkingPlaceBehavior.cpp`.

## Foundations
