# Production

## Mechanism

Making units and buildings, and gathering what pays for them, as RTS games do; also base building and MMO housing. Its parts are the `train`, `build` and `gather` kinds ([Kinds](actions.md#kinds)), as StarCraft II's ability classes, supply, requirements, and player modifiers for upgrades ([Modifiers](stats.md#modifiers)). Its decisions are [RTS foundations](../11-rts-foundations.md)'s.

## Data

- **Mode:** `[supply] max`, a whole number: the mode counts supply, no player's cap passing `max`. Without it, the load refuses a `supply` section.
- **Unit type:** `production = { queue, exit, rally }`, its train queue's length, 1 to 255, and its exit and default rally points, each `[x, z]` in meters in its own frame, within 2²⁰ m, none when absent; `supply = { cost, provides }`, whole numbers, 0 by default; `node = { resource, amount }`, a node of a player resource holding a positive whole amount; `drop_off = { resources }`. A building is a type with a box body.
- **`train`:** `name`, which a client shows on its producer's button, and the load requires, as it requires a `build`'s; `unit_type`, a type of its package that walks; its cost; its time in `windup_ms`; its cooldown; `requires`. It takes no target.
- **`build`:** `unit_type`, a type of its package with a box body; `targeting = "point"`; `range`, how near its builder comes to the box; its cost; its time in `windup_ms`; `construct`: `"alone"`, `"builder"`, or `{ builders = [...] }`, a positive rate for each count of builders from one; `start`, `"arrival"`, the default, or `"order"`, when the build starts; `start_life`, the share of the life's maximum a site starts with, above 0 and at most 1, needed when the type has a life; `cancel_refund`, the share of its player resources a cancel returns, all by default; `placement = { near, away }`, lists of `{ filter, distance }`; `requires`.
- **`gather`:** `targeting`, a filter of its nodes; `resource`; `take`, the most a trip carries, positive; `windup_ms`, a trip's gathering time; `range`, to a node and a drop-off; `bounce`, how far from its node it looks for another.
- **`requires`**, on a train or a build: `{ units = [...], modifiers = [...] }`, unit types and player modifiers of the action's package.

## Rules

### Train queues

A train order passes the core's checks in Act, with a place in its queue, its requirements and, with supply, room under its player's cap; it pays its whole cost at once, as StarCraft II does when it queues a unit, keeps the player resources it paid with its entry, goes on cooldown and joins the queue. The unit stays free: a train is never under way. The queue makes one at a time; an entry's time runs from the tick it reaches the head, and when it ends, in Mode, as a timer fires, the unit type spawns of the producer's team and player, and the next entry starts in the same tick. A dead producer's queue waits and holds its supply; one that despawns takes its queue, refunding nothing. Trains that compete for a player's resources start by their producers' stable ids.

- **Spawn place.** A trained unit spawns at the center of the cell nearest a point among those its walker may stand in, as a teleport finds its cell ([Forced movement](navigation.md#forced-movement)), so it stands outside every static body; with no grid, or no such cell, at that point when it is an exit point, else at its producer's position. The point is the producer's exit point, when its type names one; else that of the producer's body nearest the rally point: a box's nearest point, or a circle's edge toward it; with no rally point, or one at a unit gone or dead, the producer's position. An exit or default rally point lies in the producer's frame: its `x` turns as the first half edge of the producer's box turns by its angle, its `z` as the second, each rounded once, and the point is taken into the bounds ([Zero Hour production](../18-zero-hour-production.md#decisions), D2).
- **Rally point.** A `rally` order sets a producer's rally point, a point in the bounds, a unit, or none, as the tick's inputs apply, for a producer its player controls, dead or not. A trained unit walks to the point; to a node one of its gather actions takes, it gathers there by the first such action; to any other unit, it walks where that unit stands as it spawns; with none, or one at a unit gone or dead, it walks to its producer's default rally point, and with none of that either, it stands.
- **Cancel.** `cancel_train` names a queue entry by its place as the order applies, as the tick's inputs do; a place past the end is ignored. The entry leaves, its player resources come back and its supply frees; the pools it paid stay spent. Cancelling the head starts the next at once. A refund that would carry an amount past the largest a player's amount holds refuses the cancel, so no refund is lost.

### Supply

A player's supply used is the `cost` of the living units it owns and of the types its queued trains make; its cap the `provides` of its living, complete units, at most `max`; a unit with no owner counts for no one. A train joins a queue only when what is used, with its cost added, stays within the cap, and holds that supply until it spawns or is cancelled. A cap that falls below what is used kills nothing and stops trains; a unit a script or effect spawns counts with no check.

Supply is derived: production counts each player's sums when a tick's first train checks its room, adding each train that joins after, and the script view counts them as it reads. Nothing keeps a sum between counts, so no death, despawn, cancel or restore can leave one behind the units; an update at each event would need every system that spawns or kills to report it.

### Requirements

`requires` passes when the player owns a living, complete unit of each type it names and holds each player modifier it names; a train checks it as it joins a queue, a build as it starts, and a requirement lost later stops nothing. Tech is these and player modifiers: an upgrade is a player modifier the mode's script gives.

### Construction

A `build` order aims at a point, with an angle in degrees, or at a site. It checks its placement, requirements and cost as it applies and again as the build starts; a build that fails, or whose route ends short of its range, ends its order and pays nothing. A builder walks to its building's box's nearest point and stops in range. Any other order, or death, ends a build order. Construction needs navigation, which tests a box for room.

- **At a point.** In Act of the tick the builder comes in range, or, for a build of `start = "order"`, of the tick its order applies, wherever its builder stands ([Zero Hour production](../18-zero-hour-production.md#decisions), D1), the build starts if its placement, checks and requirements pass and its player affords its cost: it pays, goes on cooldown, and the building spawns there, of the builder's team and player, as a **site**, through the mode's spawner, with its life at `start_life` of its maximum, rounded down, and at least the least amount above 0. Builds start by their builders' stable ids, and each one's placement sees the sites placed before it, so two builders that arrive in one tick at one place place one site. With `alone` the builder is free; with `builder` or `builders` it stays as the site's builder, and the builder of an order build walks to the site from where it stood.
- **At a site.** A build aimed at a site of its own type and player joins it and pays nothing, as an SCV resumes a building. An `alone` site takes no builder. A `builder` site is held by one builder, the one that placed it or the first that came; another takes it only once that one stops building it, and its order ends else.
- **Progress.** Each tick, in Mode before the trains finish, each site by stable id adds its rate to its progress: 1 for `alone`; for `builder`, 1 while its builder builds; for `builders`, the table's entry for the count of builders that build, the last entry past the table, 0 for none. A builder builds while it lives, its order is this site, its body is within its range of the site, and no tag blocks its `use` group. Progress counts ticks in a `Num`; the site completes in the tick its progress reaches `windup_ms` in ticks, rounded up, and then loses its site state and its `constructing` tag, and its builders' orders end.
- **Life.** A site gains its life as it progresses and keeps the damage it takes, as in StarCraft II. With `g` its maximum less the life it started with and `T` its time, a tick that moves its progress from `p` to `q` adds `g·q/T` less `g·p/T`, each exact and rounded down, within the maximum, so a site that completes has gained exactly `g`.
- **A site** has the engine tag `constructing`, which blocks its attacks, casts and uses; it gives no supply, meets no requirement and takes no load; it may be attacked and dies with no refund.
- **Cancel.** `cancel_build` on a living site returns `cancel_refund` of each player resource it paid, rounded down, and despawns it with no death; its builders' orders end.

### Placement

A placement passes when its box lies within the bounds and overlaps no wall of its layer, no body of a unit of its layer that cannot walk, and no body of a walker of its layer whose team is not friendly to the builder's and that the builder's group sees ([RTS foundations](../11-rts-foundations.md#decisions), D8); and when it meets `placement`: within each `near` entry's `distance` of a living unit its filter selects, as a Protoss pylon's field, and within no `away` entry's, as a town hall keeps from minerals. Shapes overlap when their insides share a point, by exact cross products, so two buildings may touch. Distances follow the reach rule. As the building spawns, each walker of its layer the box overlaps goes at once to the nearest cell it may stand in, by stable id, as a teleport places a unit, disjointing nothing and ending its forced move; the grid holds the new box, so it lands clear. A placement refuses for walls and static bodies seen or not, and for a walker only when the player sees it, so a refusal never tells of a hidden unit.

### Gathering

A `gather` order aims at a node its filter selects, of its resource. The worker loops, in Act after the trains start, workers by stable id, with the engine tag `gathering`:

1. **To the node.** It walks into range. A node no other worker holds is then its own, and it gathers for the action's time, rounded up, at least a tick. A held node sends it to the nearest free living node of the same resource its filter selects within `bounce` of this one, nearest by the distance to the node's body, the lower stable id on a tie; with none, it waits at the node.
2. **Gather.** When its time ends it takes `take`, or what is left, as its load. An empty node is gone to the loop at once and despawns as the tick ends; each worker whose node is gone looks for another within `bounce` of where it stood, as its loop next steps, or after delivering its load, and stops with none.
3. **To a drop-off.** It walks to the nearest living, complete unit of its player whose `drop_off` takes the resource, chosen as it sets out and again if that one is gone. In range, its load joins its player's resource, and it goes back to its node, or to the nearest free node within `bounce` of where its node was. With no drop-off it stands with its load; with no node it stops. A load that would carry its player's amount past the largest it holds stays with the worker, so no load is lost.

A step that ends a gather, chooses a drop-off, delivers or moves to another node is followed in the same tick by the next, up to five a tick. A node frees as its worker's gather ends and, as the loop next runs, when its worker's loop ends or it dies; its first waiting worker, by the tick it began to wait, then by stable id, takes it, its gather timed from that tick. A worker whose tags block `use` takes no step, and a gather it is in ends with no load and frees its node; it starts again from step 1 as the block ends.

A `gather` aimed at a drop-off of its player that takes its load, by a worker with a load, delivers it and goes back to its last node, as StarCraft II's return cargo. A worker sent to gather with a load of that resource delivers it first; with a load of another, it drops it. Any other order ends the loop, and the worker keeps its load. Two gathering units do not collide or steer round each other ([RTS foundations](../11-rts-foundations.md#decisions), D4).

Selection and control groups are client-side; a unit a `rally`, `build` or `gather` aims at follows the rule of every order's unit target ([Orders](control.md#orders)).

## State and derived

- **State:** each queue and the player resources each entry paid; each rally point; each site's progress, its life to gain, what its build paid, and the builder that holds a `builder` site; each builder's order; each node's amount and the worker that holds it; each worker's step, node, drop-off, wait tick and load. Players' resources are the core's, their modifiers `stats`'s.
- **Derived:** each player's supply used and cap, and its complete units of each required type, from the units and the queues.

## Script API

`ctx.supply_used(player)` and `ctx.supply_cap(player)`, as the script view read the match, failing in a mode with no supply; `unit.load`, 0 with none; the engine tags `constructing`, `gathering`, `node` and `drop_off`. An RTS mode pays with `ctx.add_resource(player, name, amount)` and upgrades with `ctx.add_player_modifier(player, id)`. A train's, a build's and a gather's hooks and effect lists wait.

## Network

A player's supply and resources go to that player, or as the mode sets, as components of its player entity: the server derives supply as each tick ends and sends it, as a client holds only the units it sees. A train queue goes the same way as a component of its producer, of the kind `queues`; loads and sites as their units' ([Replication audiences](../14-replication-audiences.md), D7, D9). A builder's order, a worker's loop and a site's progress go to their owner, of the kind `prediction`, which its client predicts them by; a node's amount and holder, and the tags `constructing` and `gathering`, go to everyone who sees the unit. A client reads each node as the server sent it and writes none: its worker carries what a take would take, and the node's amount and holder stay the server's word. Only the server derives supply and despawns a node that ran out. A client's placement tests the units it holds, each as seen, where the server last had them (D12).

## Cost

Each queue a counter a tick; each site a progress step and a count of its builders; each worker in the loop up to five steps, a hold reading nothing new, a pass over the nodes when its node is held or gone, and over the workers when a node frees; each placement its box's tests against the walls, the static index and the bodies near; supply and requirements a pass over the owned units in a tick that starts a train, and in each script view read with supply. `integration/gather/two_fields` runs 200 workers on 16 nodes and two drop-offs.

## Genres

StarCraft and C&C Generals; base building in survival games; MMO housing.
