# Production

## Mechanism

- **Train, build and gather are action kinds.** Making units and buildings and gathering what pays for them are the `train`, `build` and `gather` kinds, as StarCraft II's ability classes, with supply, requirements, and player modifiers as upgrades.

## Train queues

- **A train pays at once.** A train pays its whole cost as it joins its queue, as StarCraft II does when it queues a unit, and keeps what it paid with its entry for a cancel.
- **A train is never under way.** The producer stays free; the queue makes one at a time, and its unit spawns of the producer's team and player.
- **A producer may name its exit and rally.** A unit type's exit and default rally points lie in its own frame and turn with it; a trained unit spawns at the free cell nearest the exit, as a teleport finds its cell, so it stands outside every static body, and walks to its rally point, or gathers there when it is a node.
- **No refund is lost.** A cancel whose refund would pass the largest amount a player holds is refused.

## Supply

- **Supply is derived, not a player resource.** Supply used and the cap follow from the living units, the queued trains and the complete providers, counted when a train checks its room, so no script keeps it and it cannot drift from them; an update at each event would need every system that spawns or kills to report it.

## Requirements

- **Requirements are checked at the start.** A train checks them as it joins its queue, a build as it starts, and a requirement lost later stops nothing; tech is requirements and player modifiers.

## Construction

- **Three styles, chosen by the build action.** `alone`, as StarCraft II's Protoss buildings grow; `builder`, one builder for the whole time, as a Terran SCV builds; `builders`, a rate for each count of builders, as Age of Empires and 0 A.D. speed a site less for each one more. A table is exact and states any curve: 0 A.D.'s n^0.7 has no exact form in fixed point, so a mode writes it rounded.
- **A build starts on arrival or on order.** An arrival build starts when its builder comes in range, as StarCraft II's SCV; an order build pays and places its site in the tick its order applies, wherever its builder stands, as Zero Hour's dozer does, and the builder then walks to it.
- **Two builders, one site.** Builds start by their builders' stable ids, and each sees the sites placed before it, so two builders that arrive at one place in one tick place one site.
- **A site gains its life as it progresses,** keeping the damage it takes, as in StarCraft II, its gains exact so a complete site has gained exactly its maximum.
- **A site is not yet a building.** It gives no supply, meets no requirement, and its tag blocks its attacks and casts.

## Placement

- **A seen enemy blocks, a friendly or hidden one moves out.** A placement fails where its box overlaps a wall, a static body, or a walker of a team not friendly to the builder's that the builder's group sees. A refusal for a hidden walker would tell the builder it is there, so a hidden enemy is moved out as a friendly walker is, at the cost that a player may move a hidden enemy by building on it.
- **A walker in the box goes to the nearest free cell,** as a teleport places a unit: collision would push a walker caught between the new box and another static body from one into the other every tick.
- **Near and away rules.** A build may need to stand near a unit, as a Protoss pylon's field, or away from one, as a town hall keeps from minerals.

## Gathering

- **One gatherer at a node.** A worker that finds its node taken goes to the nearest free node of the same resource near it, or waits, as a StarCraft II worker moves to another mineral patch.
- **Gatherers pass through each other,** so the income does not depend on the order collision parts them in.
- **Return cargo.** A gather aimed at a drop-off delivers the load and goes back, as StarCraft II's return cargo.
- **No load is lost.** A load that would pass the largest amount a player holds stays with its worker.

## Network

- **Only the server spawns.** A predicting client queues a train and starts a build as the server does, and spawns neither, as the net layer matches no unit a client spawns to the server's.
- **Supply comes from the server,** as a client holds only the units it sees.
