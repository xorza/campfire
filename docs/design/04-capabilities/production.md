# Production

## Mechanism

Making units and buildings, and gathering what pays for them, as RTS games do; also base building in survival games and MMO housing. Its parts are the `train`, `build` and `gather` action kinds ([Kinds](actions.md#kinds)), as StarCraft II's are ability classes, supply, requirements, and player modifiers for upgrades ([Modifiers](stats.md#modifiers)). Its first cut is the [RTS skirmish](../11-rts-skirmish.md)'s.

## Data

- **Mode:** `[supply] max`, a whole number: the mode counts supply, and no player's cap passes `max`. A mode without it has no supply, and the load refuses a `supply` section.
- **Unit type:** `production = { queue }`, the most entries its train queue holds, 1 to 255. `supply = { cost, provides }`, whole numbers, each 0 by default: what a unit of the type uses, and what a complete one gives. `node = { resource, amount }`: a node of a player resource of the mode, holding `amount`, a positive whole number. `drop_off = { resources }`: the player resources it takes. A building is a type with a box body ([Bodies](00-overview.md#space-and-map)).
- **`train` action:** `unit_type`, a type of the action's package that walks; its cost in pools and player resources; its time in `windup_ms`; its cooldown; `requires`. It takes no target.
- **`build` action:** `unit_type`, a type of the action's package with a box body; `targeting = "point"`; its `range`, how near its builder comes to the box; its cost; its time in `windup_ms`; `construct`, `"alone"`, `"builder"`, or `{ builders = [...] }`, the rate for each count of builders from one, each a positive number, at least one entry; `start_life`, the share of its life pool's maximum a site starts with, above 0 and at most all, needed when the type has the mode's life pool; `cancel_refund`, the share of its player resources a cancel returns, all by default; `placement = { near, away }`, each a list of `{ filter, distance }`; `requires`.
- **`gather` action:** `targeting`, a filter of the nodes it gathers; `resource`, a player resource of the mode; `take`, the most a trip carries, a positive whole number; `windup_ms`, the time a trip gathers; `range`, how near it comes to a node and to a drop-off; `bounce`, meters, how far from its node it looks for another.
- **`requires`**, on a train or a build: `{ units = [...], modifiers = [...] }`, unit types and player modifiers of the action's package.

## Rules

### Train queues

A train order passes the core's checks in Act, the trains first of production's starts, then the builds, then the gather loops, each in order of its unit's stable id, with a place in its unit's queue, its requirements and, with supply, room under its player's cap among them; it pays its whole cost at once, as StarCraft II does when it queues a unit, keeps the player resources it paid with its entry, goes on cooldown and joins the queue. The unit stays free: a train is never under way. The queue makes one at a time, in order; an entry's time runs from the tick it reaches the head, and when it ends, in the Mode stage at the end of the tick its time ends in, as a timer fires, the unit type spawns, of the producer's team and player, and the next entry starts in the same tick. A dead producer's queue waits, and holds its supply; a producer that despawns takes its queue with it, refunding nothing. Trains that compete for a player's resources start in the order of their producers' stable ids.

- **Spawn place.** A trained unit spawns at the center of the cell nearest a point, among the cells its walker may stand in, by the rule a teleport finds its cell ([Forced movement](navigation.md#forced-movement)), so it stands outside its producer's box and every other static body; on a grid with no such cell, at its producer's position. The point is that of its producer's body nearest the rally point, each coordinate rounded toward negative infinity, or the producer's position with no rally point.
- **Rally point.** A `rally` order sets a producer's rally point: a point, a unit, or none. A unit a train makes walks to the point; to a node whose resource one of its gather actions takes, it gathers there, by the first such action in its slots; to any other unit, it walks to where that unit stands as it spawns; with none, or a unit gone or dead, it stands.
- **Cancel.** A `cancel_train` order names an entry of its producer's queue by its place, as the queue stands when the order applies; a place past its end is ignored. The entry leaves the queue, its player gets back the player resources the entry paid, and its supply is free; the pools it paid stay spent. Cancelling the head ends its time, and the next entry starts in the same tick. A refund that would carry an amount past the largest a player's amount holds refuses the cancel, and the entry stays, so no refund is lost.

### Supply

In a mode with `[supply]`, a player's supply used is the sum of the `cost` of the living units it owns and of the unit types its queued trains make, and its cap the sum of the `provides` of the living, complete units it owns, at most `max`; a unit with no owner counts for no player. A train joins a queue only when what is used, with its unit type's cost added, is at most the cap, and holds that supply until it spawns or is cancelled. A cap that falls below what is used kills nothing, and stops trains until it rises. A unit that a script or an effect spawns counts with no check, so what is used may pass the cap.

Supply is derived: production keeps each player's two sums as each unit and each queue entry starts or stops counting, a constant cost each, and a restore counts them again from the units and the queues. A check after each tick of the tests counts them from the units and compares, so a change that misses one fails a test in its tick.

### Requirements

An action's `requires` passes when its player owns a living, complete unit of each type it names, and holds each player modifier it names. A train checks it as it joins a queue, a build as it starts; a requirement lost later stops nothing already queued or placed. Production keeps a count of each player's living, complete units of each type some requirement names, derived as supply is. Tech is these and player modifiers: an upgrade is a player modifier on the units it selects, which the mode's script gives, and which a requirement may name.

### Construction

A `build` order aims at a point, with an angle in degrees, or at a site. It is checked as it applies, its placement, its requirements and its cost against the match as it stands, and again as the build starts; a build that fails either check, or whose route ends short of its range, ends its order and pays nothing.

- **At a point.** The builder walks until the box its building would have there, at that angle, comes within its action's range. In Act of the tick it arrives, the build starts if its placement passes, the action's checks and its requirements pass, and its player affords its cost: it pays the whole cost, goes on cooldown, and the building spawns there, of the builder's team and player, as a **site**, through the mode's spawner, with its life pool at `start_life` of its maximum, rounded down to a `Num`, and at least the least amount above 0. Builds start in the order of their builders' stable ids, and each one's placement sees the sites the builds before it placed, so two builders that arrive in one tick at one place place one site. With `alone` the order ends, and the builder is free. With `builder` or `builders`, the builder stays as the site's builder.
- **At a site.** A build aimed at a site of its own building type and its player joins it, as an SCV resumes a building: the builder walks into range and builds, and pays nothing. An `alone` site takes no builder; a `builder` site takes one only while it has none.
- **Progress.** Each tick, in the Mode stage before the trains finish, each site in order of stable id adds its rate to its progress: 1 for `alone`; for `builder`, 1 while its builder builds, and 0 else; for `builders`, the table's entry for the count of its builders that build, the last entry for a count past the table, and 0 for none. A builder builds while it lives, its order is to build this site, its body is within its action's range of the site, and no tag blocks its `use` group. Progress counts ticks in a `Num`; the site completes in the tick its progress reaches its time, `windup_ms` in ticks rounded up. Then it is a building: it loses its site state and its `constructing` tag, and its builders' orders end.
- **Life.** A site gains its life as it progresses, and keeps the damage it takes, as a building grows in StarCraft II. With `g`, its pool's maximum less the life it started with, fixed as it spawns, and `T` its time, a tick that moves its progress from `p` to `q` adds `g·q/T` less `g·p/T`, each computed exactly and rounded down to a `Num`, `q` at most `T`, within the pool's maximum, so a site that completes has gained exactly `g`, with no rounding carried from tick to tick.
- **A site.** A site has the engine tag `constructing`. No action of its starts; it gives no supply, meets no requirement, and takes no load; it may be attacked, and it dies as any unit does, with no refund.
- **Cancel.** A `cancel_build` order to a site returns `cancel_refund` of each player resource its build paid, each rounded down, and despawns the site, with no death; its builders' orders end.

### Placement

A placement passes when its box lies within the map's bounds and overlaps no wall of its layer, no body of its layer of a unit that cannot walk, and no body on its layer of a unit that walks, whose team is not friendly to the builder's, and that the builder's vision group sees ([RTS skirmish](../11-rts-skirmish.md#decisions), D8); a unit with no body blocks nothing; and when it meets `placement`: for each entry of `near`, the box is within `distance` of a living unit its filter selects for the builder, as a Protoss building needs a pylon's field; for each entry of `away`, of none, as a town hall keeps its distance from minerals. Two shapes overlap when their insides share a point, decided exactly by the signs of cross products; touching is not overlap, as in collision, so two buildings may stand side by side. Every distance is the reach rule's ([Space and map](00-overview.md#space-and-map)). A friendly walker, or an enemy one the builder's group does not see, may stand in the box: as the building spawns, each walker of its layer whose body the box overlaps goes at once to the nearest cell it may stand in, as a teleport places a unit, in order of stable id; the move disjoints nothing, and ends the forced move under way. The grid it lands on has the new box in it, so it lands clear of every static body, and no push of collision can hold it between two of them. A placement refuses for the walls and the static bodies whether its player sees them or not, as no placement can move them; it refuses for a walker only when its player sees it, so a refusal never tells a player of a unit its group does not see, and building over a hidden walker moves it out.

### Gathering

A `gather` order aims at a node that its action's filter selects and whose resource is its action's. The worker then loops, in Act after the trains start, workers in order of stable id, and has the engine tag `gathering` while it does:

1. **To the node.** It walks until the node comes within its action's range. A node no other worker holds is then its own: it holds it, and gathers for the action's time, in ticks rounded up, at least one. A node another worker holds sends it to the nearest living node of the same resource that its action's filter selects, whose body comes within `bounce` of this node's and that no worker holds, nearest to the worker by the reach rule's distance, the lower stable id on a tie; with none, it waits at the node.
2. **Gather.** When its time ends, it takes `take` from the node's amount, or what is left, and carries it as its load. A node with nothing left despawns at the tick's end, and its waiting workers look for another as in step 1, and stop with none.
3. **To a drop-off.** It walks to the nearest living, complete unit of its player whose `drop_off` takes the resource, by the distance from its body's edge to the drop-off's, the lower stable id on a tie, chosen as it sets out and again if that one is gone. In range, its load joins its player's resource, and it goes back to its node, or, with its node gone, to the nearest free node of the resource within `bounce` of where its node was. With no drop-off it stands with its load; with no node it stops. A load that would carry its player's amount past the largest an amount holds stays with the worker, which stands, so no load is lost.

A node frees when its worker's gather ends, when its worker's loop ends, and when its worker dies. As it frees, its first waiting worker, by the tick it began to wait, then by stable id, holds it, and its gather's time runs from that tick, whichever system freed it; so a worker's place in the wait is never lost to one the stage visits first. A worker whose tags block its `use` group takes no step of the loop, and a gather it is in ends with no load and frees its node; it starts again from step 1 as the block ends.

A `gather` order aimed at a drop-off of its player that takes its load's resource, by a worker with a load, takes the load there and goes back to its last node, as StarCraft II's return cargo does. A worker with a load of the resource it is sent to gather takes it to a drop-off first; one with a load of another resource drops it. Any other order ends the loop, and the worker keeps its load. Two units that both have the `gathering` tag do not collide, and do not steer round each other ([RTS skirmish](../11-rts-skirmish.md#decisions), D4).

Selection and control groups are client-side; orders name the units they go to. A unit that a `rally`, a `build` or a `gather` aims at follows the rule of every order's unit target ([Issue log](../../issues/control.md)).

## State and derived

- **State:** each queue, and the player resources each entry paid; each rally point; each site's progress, its life to gain, and the player resources its build paid; each builder's order; each node's amount and the worker that holds it; each worker's step of the loop, its node, its drop-off, the tick it began to wait, and its load. The players' resources are the core's, and their modifiers are `stats`'s ([Modifiers](stats.md#modifiers)).
- **Derived:** each player's supply used and cap, and its count of complete units of each type a requirement names, from the units and the queues.

## Script API

`ctx.supply_used(player)` and `ctx.supply_cap(player)`; `unit.load`, the amount it carries, 0 with none; the engine tags `constructing`, `gathering`, `node` (a type with a `node` section) and `drop_off` (one with a `drop_off` section). An RTS mode pays with the core's `ctx.add_resource(player, name, amount)` and upgrades with `stats`' `ctx.add_player_modifier(player, id)`. Its kinds run from data: a train's, a build's and a gather's hooks and effect lists wait.

## Network

A player's queues, supply, loads and resources go to that player and, as the mode sets, to the team. No client receives them yet; Stage 8 sends them ([Issue log](../../issues/net.md)).

## Cost

Each queue costs a counter a tick; each site a progress step and a count of its builders; each worker in the loop one step of its state machine, and a search of the body grid within `bounce` when its node is held; each placement its box's tests against the walls, the static index and the bodies near it; supply and requirements a constant update for each event that changes them. The bench `production/gather`, 200 workers on 16 nodes and two drop-offs, measures the loop when it lands.

## Genres

StarCraft and C&C Generals; base building in survival games; MMO housing.
