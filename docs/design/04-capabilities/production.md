# Production

## Mechanism

- **Train, build and gather are action kinds.** Making units and buildings and gathering what pays for them are the `train`, `build` and `gather` kinds, as StarCraft II's ability classes, with supply, requirements, and player modifiers as upgrades.

## Train queues

- **A train pays at once.** A train pays its whole cost as it joins its queue, as StarCraft II does when it queues a unit, and keeps what it paid with its entry for a cancel.
- **A train is never under way.** The producer stays free; the queue makes one at a time, and its unit spawns of the producer's team and player.
- **A producer may name its exit and rally.** A unit type's exit and default rally points lie in its own frame and turn with it; a trained unit spawns at the free cell nearest the exit, as a teleport finds its cell, so it stands outside every static body, and walks to its rally point, or gathers there when it is a node.
- **No refund is lost.** A cancel whose refund would pass the largest amount a player holds is refused.
- **A trained unit runs a hook.** A unit a train makes runs `on_unit_trained` with its producer, as Warcraft III fires `EVENT_PLAYER_UNIT_TRAIN_FINISH`, queued on the server as it spawns.

## Supply

- **Supply is derived, not a player resource.** Supply used and the cap follow from the living units, the queued trains and the complete providers, so no script keeps it and it cannot drift from them. Refused: an update at each event, which would need every system that spawns or kills to report it.
- **Supply and holdings follow what changed.** One cache keeps what each unit counts for, and its observers mark a unit whose owner, type, death, site or presence comes or goes, as each fires on every path that spawns, kills, restores or rolls back; a queue, which changes in place, shows by its change tick. Each reader follows the marked units before it reads, so a count costs the units that changed, not every unit. Refused: a count of every unit at each train and again at each tick's end, and a `Changed` filter on each part, which reads every unit's change ticks in each tick.

## Requirements

- **Requirements are checked at the start.** A train checks them as it joins its queue, a build as it starts, and a requirement lost later stops nothing; tech is requirements and player modifiers.
- **A requirement may be any one of several types.** An entry of `requires.units` is a type or a list of types, and a list holds when the player holds any one of them, as Zero Hour's prerequisite line `Object = A B` (`UNIT_OR_WITH_PREV`) and StarCraft II's requirement nodes of "or". Refused: OpenRA's prerequisite tokens that buildings provide, which make every building of an alternative name a token it provides, a second name for one fact.

## Construction

- **Three styles, chosen by the build action.** `alone`, as StarCraft II's Protoss buildings grow; `builder`, one builder for the whole time, as a Terran SCV builds; `builders`, a rate for each count of builders, as Age of Empires and 0 A.D. speed a site less for each one more. A table is exact and states any curve: 0 A.D.'s n^0.7 has no exact form in fixed point, so a mode writes it rounded.
- **A build starts on arrival or on order.** An arrival build starts when its builder comes in range, as StarCraft II's SCV; an order build pays and places its site in the tick its order applies, wherever its builder stands, as Zero Hour's dozer does, and the builder then walks to it.
- **Two builders, one site.** Builds start by their builders' stable ids, and each sees the sites placed before it, so two builders that arrive at one place in one tick place one site.
- **A site gains its life as it progresses,** keeping the damage it takes, as in StarCraft II, its gains exact so a complete site has gained exactly its maximum.
- **A site is not yet a building.** It gives no supply, meets no requirement, and its tag blocks its attacks, casts, uses and trains.
- **A site keeps its build's time.** A site holds the time its build gives at its rank from its placement, so its growth and a script's read of its share look up no book, and a restore checks the time against the build.
- **A building may come with its units.** A producer's `free` lists the unit types of its package that leave its exit once, on the server, as its site completes, as OpenRA's `FreeActor` gives a refinery its harvester and Zero Hour's `SpawnBehavior` of `OneShot` a supply center its gatherer ([Zero Hour supply](../../research/zero-hour-supply.md)). Refused: the mode script's `on_site_complete`, which can spawn a unit but not send it through an exit or order it.
- **A building that spawns whole frees its units.** A producer that spawns whole, not as a site, sends its free units and its kept spawns as it spawns, by production's spawn callback, which only the server's spawner runs, never as a restore or a client brings it back, as Zero Hour's `SpawnBehavior` acts as its object is made, whatever made it, and StarCraft II's `UnitCreate` makes a unit whole unless asked to construct it ([Test matches](../../research/test-matches.md)). Refused: free units of a site alone, under which a base start's supply center frees no gatherer.
- **A building may keep spawns.** A unit type's `spawns = { unit_types, count, replace_ms }`, taken in turn ([Spawn leaders](../spawn-leaders.md)), leave its exit as its site completes, each that dies comes back `replace_ms` later, and they die with it, as Zero Hour's `SpawnBehavior` of `SpawnedRequireSpawner` keeps the Stinger site's soldiers; a producer of no exit sends them from the free cell nearest its center. Refused: the mode script's `on_unit_died`, which every game would write again; `free` units, which leave once and are never replaced.
- **A unit may lead its spawns, planned.** A type may lead its `spawns`: of no life, body or walk of its own, it stands at their centre, an order to it is theirs, and it dies with the last, as Zero Hour's angry mob and OpenRA's port of it ([Spawn leaders](../spawn-leaders.md)).
- **A complete site runs a hook.** A site that completes runs `on_site_complete`, as Warcraft III fires `EVENT_PLAYER_UNIT_CONSTRUCT_FINISH`, queued on the server by stable id in its tick; a predicting client grows its sites alike and runs no hook.
- **A build point its team sees.** A placement needs its point in a cell its builder's team saw in the last Vision stage, as Zero Hour's `isLocationLegalToBuild` refuses a point under shroud first, so a build into the fog finds out nothing of what stands there; the fog a restored match holds is filled again as its first tick starts, from the units and reveals the last stage saw.
- **Tags block production.** A train starts only when its producer's tags do not block `produce`, as a cast starts only when they do not block `cast`; each kind of action names the one block that stops it, so a stun that a mode declares can stop a queue or leave it running.

## Placement

- **A building may stand on a circle.** A build's body is a box or a circle, and its placement tests that shape against the walls, the map's bounds and the bodies, and its builders walk to the shape's nearest point, as Zero Hour's pathfinder gives a cylinder structure a circle of its radius (`Pathfinder::classifyObjectFootprint`). Refused: a circle's bounding box, which blocks the corners that the original leaves free and makes the tunnel network 50 m square.
- **A seen enemy blocks, a friendly or hidden one moves out.** A placement fails where its body overlaps a wall, a static body, or a walker of a team not friendly to the builder's that the builder's group sees. A refusal for a hidden walker would tell the builder it is there, so a hidden enemy is moved out as a friendly walker is, at the cost that a player may move a hidden enemy by building on it.
- **A walker in the box goes to the nearest free cell,** as a teleport places a unit: collision would push a walker caught between the new body and another static body from one into the other every tick.
- **Near and away rules.** A build may need to stand near a unit, as a Protoss pylon's field, or away from one, as a town hall keeps from minerals.
- **Production asks navigation through units.** A placement reads the walls from `units`' book of them, which navigation builds from its map section, and tests the static bodies through the unit grid; a trained unit's free cell comes from a one-shot system `units` declares and navigation registers. Refused: production depending on navigation for two reads, and the pathing grid's static index and ring search moved down into `units`, which would hold navigation's pathing types.

## Gathering

- **A gather search scans.** A worker's search for a node and for a drop-off visits the nodes and drop-offs, as Zero Hour's `findBestSupplyWarehouse` scans the player's warehouses, and a freed node finds its next worker among the waiting ones. Refused: a search of the unit grid and a queue on each node, for a tick of 200 workers at 16 nodes that costs 20.5 µs ([Gather loop](../research/gather-loop.md)).
- **One gatherer at a node.** A worker that finds its node taken goes to the nearest free node of the same resource near it, or waits, as a StarCraft II worker moves to another mineral patch.
- **Gatherers pass through each other,** so the income does not depend on the order collision parts them in.
- **Return cargo.** A gather aimed at a drop-off delivers the load and goes back, as StarCraft II's return cargo.
- **A gather takes its load a unit at a time.** A gather's `unit` is what each windup takes, up to its `take`, so a trip from a nearly empty node is short; with no `unit`, one windup takes the whole load. Practice: Zero Hour's box each `SupplyWarehouseActionDelay`, OpenRA's bales ([Zero Hour supply](../../research/zero-hour-supply.md)). Refused: a whole trip's windup, which pays a full trip for the last box of a dock.
- **A drop-off takes its time.** A gather's `deliver_ms` is how long a worker stands at the drop-off before its load joins its player's, Zero Hour's `SupplyCenterActionDelay`; one pushed off walks back and stands its whole time again. Refused: a delay on the drop-off, as Zero Hour's differs by gatherer, 150 ms for a worker and 400 ms for a truck.
- **An empty node may stay.** A node's `empty` is `goes`, the default, or `stays`, as Zero Hour's supply dock stays and its pile goes (`DeleteWhenEmpty`); no worker gathers an empty node.
- **A producer may start its gatherers.** A producer's `gathers` sends each gatherer that leaves its exit, trained or freed, to the free node nearest it within its first gather's `bounce`, as a unit that leaves Zero Hour's `SupplyCenterProductionExitUpdate` is forced into wanting after its exit path; a node the producer rallies to comes first, and a node found wins over its rally point. Refused: OpenRA's `SearchOnCreation` on the gatherer, which would start a GLA worker that a command center trains, as Zero Hour does not.
- **No load is lost.** A load that would pass the largest amount a player holds stays with its worker.
- **Production's rules as data, planned.** With the slice that needs each, its default today's behavior: capacities that the mode declares in place of one supply cap; requirements in conjunctive normal form, so Zero Hour's either-or prerequisites hold; one progress model by rate, with research entries; a node's capacity of gatherers; `passes` as a tag property of the mode; a placement on a unit, repair and builders that a build consumes; the gather and progress defaults as data.

## Network

- **Only the server spawns.** A predicting client queues a train and starts a build as the server does, and spawns neither, as the net layer matches no unit a client spawns to the server's.
- **Supply comes from the server,** as a client holds only the units it sees.
