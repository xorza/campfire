# Production

## Mechanism

Making units and buildings, and gathering what pays for them, as RTS games do; also base building in survival games and MMO housing. Its parts are the `train`, `build` and `gather` action kinds ([Kinds](actions.md#kinds)), as StarCraft II's are ability classes, and player modifiers for upgrades ([Modifiers](stats.md#modifiers)).

## Data

A `train` action: the mode's `unit_type` it makes, its cost in pools and player resources, its time in `windup_ms`, its cooldown, its requirements; it takes no target. A unit type that holds one has `production = { queue = 5 }`, the most entries its queue holds. A `build` action: the building type, its footprint on the grid, the mode's placement rules (power, creep, distance), its cost and time. A `gather` action: the node filter and the drop-off filter. A unit type's `node` section: what it holds and how much; its `drop_off` section: what it takes. The mode's player resources, among them a supply cap.

## Rules

- **Build queues:** a train order passes the core's checks in Act, with a place in its unit's queue among them; it pays its whole cost at once, as StarCraft II does when it queues a unit, goes on cooldown and joins the queue. The unit stays free: a train is never under way. The queue makes one at a time, in order; an entry's time runs from the tick it reaches the head, and when it ends, in the Mode stage at the end of the tick its time ends in, as a timer fires, the unit type spawns at its producer's position, of the producer's team and player, and the next entry starts in the same tick. A dead producer's queue waits. Trains that compete for a player's resources start in the order of their producers' stable ids. Cancel and refund are part of the kind; a rally point sends new units on.
- **Construction:** placing a building checks its footprint against the grid's occupancy and the mode's rules; the building then grows over time, or builders construct it.
- **Harvesting:** a gather action walks to a node, gathers over time, and carries the load to a drop-off, again until stopped. Nodes deplete.
- **Tech:** requirements are buildings, upgrades or levels a player owns; an upgrade is a player modifier on the units it selects.
- **Supply:** a cap on units per player, as a player resource.

Selection and control groups are client-side; orders name the units they go to.

## State and derived

- **State:** each queue, each construction's progress, each node's amount, each worker's load; the players' resources and modifiers.
- **Derived:** the grid's occupancy, from the buildings.

## Script API

`ctx.add_resource(player, name, amount)`, `ctx.add_player_modifier(player, id)`; the action hooks.

## Network

A player's queues and resources go to that player and, as the mode sets, to the team.

## Cost

Each queue costs a counter a tick; each worker its gather loop's state machine; each placement a footprint test.

## Genres

StarCraft and C&C Generals; base building in survival games; MMO housing.
