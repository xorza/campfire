# Production

## Mechanism

- **Train, build and gather are action kinds.** Making units and buildings and gathering what pays for them are the `train`, `build` and `gather` kinds, as StarCraft II's ability classes, with supply, requirements, and player modifiers as upgrades.

## Train queues

- **A train pays at once.** A train pays its whole cost as it joins its queue, as StarCraft II does when it queues a unit, and keeps what it paid with its entry for a cancel.
- **A train is never under way.** The producer stays free; the queue makes one at a time, and its unit spawns of the producer's team and player.
- **A producer may name its exit and rally.** A unit type's exit and default rally points lie in its own frame and turn with it; a trained unit spawns at the free cell nearest the exit, as a teleport finds its cell, so it stands outside every static body, and walks to its rally point, or gathers there when it is a node.
- **No refund is lost.** A cancel whose refund would pass the largest amount a player holds is refused.

## Supply

- **Supply is derived, not a player resource.** Supply used and the cap follow from the living units, the queued trains and the complete providers, so no script keeps it and it cannot drift from them. Refused: an update at each event, which would need every system that spawns or kills to report it.
- **Supply and holdings follow what changed.** One cache keeps what each unit counts for, and its observers mark a unit whose owner, type, death, site or presence comes or goes, as each fires on every path that spawns, kills, restores or rolls back; a queue, which changes in place, shows by its change tick. Each reader follows the marked units before it reads, so a count costs the units that changed, not every unit. Refused: a count of every unit at each train and again at each tick's end, and a `Changed` filter on each part, which reads every unit's change ticks in each tick.

## Requirements

- **Requirements are checked at the start.** A train checks them as it joins its queue, a build as it starts, and a requirement lost later stops nothing; tech is requirements and player modifiers.

## Construction

- **Three styles, chosen by the build action.** `alone`, as StarCraft II's Protoss buildings grow; `builder`, one builder for the whole time, as a Terran SCV builds; `builders`, a rate for each count of builders, as Age of Empires and 0 A.D. speed a site less for each one more. A table is exact and states any curve: 0 A.D.'s n^0.7 has no exact form in fixed point, so a mode writes it rounded.
- **A build starts on arrival or on order.** An arrival build starts when its builder comes in range, as StarCraft II's SCV; an order build pays and places its site in the tick its order applies, wherever its builder stands, as Zero Hour's dozer does, and the builder then walks to it.
- **Two builders, one site.** Builds start by their builders' stable ids, and each sees the sites placed before it, so two builders that arrive at one place in one tick place one site.
- **A site gains its life as it progresses,** keeping the damage it takes, as in StarCraft II, its gains exact so a complete site has gained exactly its maximum.
- **A site is not yet a building.** It gives no supply, meets no requirement, and its tag blocks its attacks, casts, uses and trains.
- **A build point its team sees.** A placement needs its point in a cell its builder's team saw in the last Vision stage, as Zero Hour's `isLocationLegalToBuild` refuses a point under shroud first, so a build into the fog finds out nothing of what stands there; the fog a restored match holds is filled again as its first tick starts, from the units and reveals the last stage saw.
- **Tags block production.** A train starts only when its producer's tags do not block `produce`, as a cast starts only when they do not block `cast`; each kind of action names the one block that stops it, so a stun that a mode declares can stop a queue or leave it running.

## Placement

- **A seen enemy blocks, a friendly or hidden one moves out.** A placement fails where its box overlaps a wall, a static body, or a walker of a team not friendly to the builder's that the builder's group sees. A refusal for a hidden walker would tell the builder it is there, so a hidden enemy is moved out as a friendly walker is, at the cost that a player may move a hidden enemy by building on it.
- **A walker in the box goes to the nearest free cell,** as a teleport places a unit: collision would push a walker caught between the new box and another static body from one into the other every tick.
- **Near and away rules.** A build may need to stand near a unit, as a Protoss pylon's field, or away from one, as a town hall keeps from minerals.
- **Production asks navigation through units.** A placement reads the walls from `units`' book of them, which navigation builds from its map section, and tests the static bodies through the unit grid; a trained unit's free cell comes from a one-shot system `units` declares and navigation registers. Refused: production depending on navigation for two reads, and the pathing grid's static index and ring search moved down into `units`, which would hold navigation's pathing types.

## Gathering

- **One gatherer at a node.** A worker that finds its node taken goes to the nearest free node of the same resource near it, or waits, as a StarCraft II worker moves to another mineral patch.
- **Gatherers pass through each other,** so the income does not depend on the order collision parts them in.
- **Return cargo.** A gather aimed at a drop-off delivers the load and goes back, as StarCraft II's return cargo.
- **No load is lost.** A load that would pass the largest amount a player holds stays with its worker.

## Network

- **Only the server spawns.** A predicting client queues a train and starts a build as the server does, and spawns neither, as the net layer matches no unit a client spawns to the server's.
- **Supply comes from the server,** as a client holds only the units it sees.
