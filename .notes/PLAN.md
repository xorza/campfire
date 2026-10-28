# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Defenses and airfields

The slice of [Defenses and airfields](../docs/design/defenses-and-airfields.md): a player of a Zero Hour skirmish kills an enemy Fire Base, Stinger Site and Battle Bus, trains no more jets than an airfield's places, sees doors open as units come out, and sees jets take off and land as the release game's. Its facts are [Zero Hour's bodies](../docs/research/zero-hour-bodies.md) and [Zero Hour's jets](../docs/research/zero-hour-jets.md); its checks are that document's.

### DA1. Hive and undead bodies

Touches the Zero Hour import and combat; it stands first, as an enemy's defenses that nothing kills block every later play of a fight.

- [ ] A Fire Base, a Stinger Site and a Battle Bus have no life (defenses-and-airfields.md, A hive is a structure; defenses-and-airfields.md, A type may live twice): the import gives life to an `ActiveBody` and a `StructureBody` alone (`fight_data.rs:299`), so no weapon targets them, no bar shows their health and nothing kills them. A `HiveStructureBody` imports as a structure; a unit type's `second_life = { life, modifier }` lets its unit live twice, and an `UndeadBody` imports to it, its modifier the `SECOND_LIFE` armor and no walk. Checks: the import's tests and the combat tests of the design's Checks; the Zero Hour check pins the bodies, its `unliving` count falls by 17, and a play kills a Fire Base and a Battle Bus, the bus once into its second life. Builds: The importer takes a game's data as it is; the engine reads it as it should be.

### DA2. A place reserved as the train queues

Touches production; it stands before the doors, as both change how a producer makes its units.

- [ ] An airfield of 4 places queues a fifth jet (defenses-and-airfields.md, A train at a place reserves it as it queues): a train `at_place` waits at its queue's end for a free place (`trains.rs:335`), and nothing refuses it as it queues (`trains.rs:104`). A train at a place joins only while a place is free of units and of queued trains, and its button is dimmed while none is. Checks: the production tests refuse a fifth jet and take it once a jet dies; the client's tests dim the button; the Zero Hour check's jet play queues no fifth. Builds: What can be forgotten is a type or a trait method with no default.

### DA3. Doors and hangars

Touches production, airfields, the client and the Zero Hour import; it stands after DA2, whose reserved place is the door of a train at a place.

- [ ] A producer's doors stay closed and its units leave as their trains end (defenses-and-airfields.md, A producer's door opens before its unit leaves; defenses-and-airfields.md, A hangar is a jet's start; 09-zero-hour.md, Doors): no state holds a door, and China's jets are made at their places, not in their hangars. A producer's `doors` give each door's phase and tick as state that every client that sees it receives, a unit leaves once its door opened, and the client draws each door's conditions; the import gives each `ProductionUpdate`'s door times and the Chinese airfield its hangars. Checks: the production tests open a door before its unit leaves, hold it open after the last one, close it, and keep it over a save and a restore; the client's tests draw a door's condition by its phase; the Zero Hour check pins the doors and the hangars, and its goldens move by each door's opening time, each move named; its bots' match still ends. Builds: A name is resolved once.

### DA4. A jet's takeoff and approach

Touches airfields and navigation; it stands last of the code, as the takeoff starts from DA3's hangar and place.

- [ ] A jet rolls at its full speed and climbs past its runway's end (defenses-and-airfields.md, A jet takes off as a jet; 09-zero-hour.md, Jets): the sortie's roll dashes at its speed (`sorties.rs:566`), and its landing sinks by its own climb, not to the airfield's approach height. The step first compares the client's takeoff with the release game's and reads `Locomotor::moveTowardsPositionWings`, then proposes the jet's mover, in navigation's `zero_hour` beside the ground movers, or the sortie's own roll, for review; then the roll gathers speed by the jet's acceleration and lifts by the ratio of the runway left, and the landing comes from the approach at its height and distance. Checks: the airfields tests of the design's Checks; the Zero Hour check's jet play pins the takeoff's ticks and height, its goldens move, each move named, and its bots' match still ends. Builds: A capability serves every game.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert kills an enemy Fire Base and a Battle Bus, sees an airfield refuse a fifth jet, sees the doors open as units come out, and sees jets take off and land as the release game's, in the client; then the slice's decisions fold into `04-capabilities/combat.md`, `04-capabilities/production.md`, `04-capabilities/airfields.md`, `02-engine-core.md` and `09-zero-hour.md`, its losses into `09-zero-hour.md`'s, and `defenses-and-airfields.md` goes.

## Foundations
