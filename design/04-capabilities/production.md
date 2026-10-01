# Production

## Mechanism

Making units and buildings, and gathering what pays for them, as RTS games do; also base building in survival games and MMO housing. Its parts are the `train`, `build` and `gather` action kinds ([Kinds](actions.md#kinds)), as StarCraft II's are ability classes, and player modifiers for upgrades ([Modifiers](stats.md#modifiers)).

## Data

A `train` action: the unit type it makes, its cost in player resources, its time, its requirements. A `build` action: the building type, its footprint on the grid, the mode's placement rules (power, creep, distance), its cost and time. A `gather` action: the node filter and the drop-off filter. A unit type's `node` section: what it holds and how much; its `drop_off` section: what it takes. The mode's player resources, among them a supply cap.

## Rules

- **Build queues:** a train action joins its unit's queue; the queue makes one at a time, in order. Progress, cancel and refund are part of the kind; a rally point sends new units on.
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
