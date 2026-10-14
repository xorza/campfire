# Navigation

## Mechanism

How units move: on layers, by routes round what blocks them, along waypoint paths, by forced movement, and apart from each other by collision. The space it works in, the metric, layers and bounds, is the core's ([Space and map](00-overview.md#space-and-map)).

## Data

- **Mode:** `[navigation] layers = ["ground", "air"]`, the first the default; with none, one unnamed layer. `spawn_tags`, the tags of the markers the mode's units spawn at, which the load checks a walker may stand on ([Map checks](#map-checks)).
- **Map:** `[navigation]` with `cell`, in meters, positive: whole cells over the bounds, at most 2²² cells; `[[navigation.walls]]`, each a `layer` (the first by default) and a simple polygon of `points`, `[x, z]` each, at least three, no edge crossing another: a wall, a cliff, the jungle's trees, on that layer only. `[[paths]]`, each a `name` and `points`.
- **Unit type:** `collision = { radius, layer }`, a circle up to 2,048 m on its layer; or `collision = { box = [w, h], layer }` for a type that does not walk on a planar map ([Bodies](00-overview.md#space-and-map)). A type without it collides with nothing and moves on the first layer. A unit that walks has the `move_speed` stat.

## Rules

### Backends

| Backend | Does |
| --- | --- |
| Grid | A* for the long and short routes; the one the release runs |
| Navmesh | Routes over level geometry, for 3D levels; later |
| Flow field | One field for many units to one goal, for RTS groups; later |
| Local steering | The default movement backend: follows the route at the unit's step, goes round units that stand, allows body blocking, in fixed point and a fixed order; works with every pathfinding backend |
| Zero Hour, `navigation::zero_hour` | Its pathfinder, and its locomotors as a movement backend, for its import's parity; later ([Zero Hour](../12-zero-hour.md#decisions), D3) |

Queries take world positions, a route is a list of waypoints and obstacles are shapes, so every movement backend works with every pathfinding backend. Buildings and doors change the map while it runs, and each backend updates its obstacles from them.

### Layers

Each layer has its own pathing grid, static bodies and collision: a unit's route goes round its layer's blocked cells and its bodies that cannot walk, and its body touches only bodies on its layer. A unit's layer is a tag, so a filter selects air or ground (`enemies:air`).

### Movement

Three kinds of avoidance, as in Dota 2: a long route around what never moves, a short route around units in the way, and collision as the last guard; all exact fixed point, in stable-id order, with a fixed work limit a tick. A unit whose tags block `move` stands and keeps its destination and route ([Tags](stats.md#tags)).

- **Pathing grid:** the bounds in `cell` cells, blocked for each kind of walker, a layer and a radius: the map's blocked cells of its layer, each cell whose centre comes closer than the radius to one of their squares, and each cell whose centre comes closer to a unit of its layer that cannot walk than the two radii, or, for a box, lies in it or within the walker's radius of it. A cell is the map's blocked cell when its centre lies inside or on a wall of its layer, by an exact test. The grid changes as a unit that cannot walk spawns or dies; the walls never change.
- **Regions:** for each kind of walker, the cells it can go between, as 0 A.D.'s hierarchical pathfinder keeps them: chunks of 64 × 64 cells, each chunk's open cells split into regions that touch along a side, regions of neighbouring chunks joined where their cells touch, and the joined regions numbered as reachable sets. Touching along a side is exact for A*'s moves, as a diagonal step needs both cells beside it open. A change of the static bodies builds again only the chunks it changed, then numbers the sets again over the regions. A blocked cell, as a start or goal may be, belongs to its open side neighbours' sets.
- **Long route:** A* for the unit's walker, eight neighbours with no corner cut, costs 10 and 14, ties by estimate then cell, then smoothing that keeps a waypoint only where the straight line on would overlap a static body or touch a cell the walker's grid blocks, each tested exactly. A goal the unit cannot stand on gives the nearest open cell; one in no reachable set of the start's gives the nearest cell in one, as 0 A.D.'s `MakeGoalReachable` does, found before the search by each region's box, so no search spreads over a region to learn it fails; ties go to the lower cell. Routes wait in the order first asked, then by stable id; a new goal keeps its place. Long and short routes share a work limit a tick, as many units as the grid has cells: a cell expanded or scanned, a straight line tested. Long routes go first; the plan that meets the limit finishes, and the rest wait.
- **Facing:** a walker faces its leg's way, from where the leg began, the waypoint it passed last or its place as its route or a detour began, to the next waypoint, so its facing holds for a whole leg whatever each step rounds. It turns toward it the shorter way round, counter-clockwise on a tie, at most its `turn_rate` over the tick rate a tick, and steps only when it then faces within 11.25° of the way; one of no turn rate faces it at once ([Units that drive](../17-units-that-drive.md#decisions), D2).
- **Walking a route:** a move order, a chase and a path set the destination; a new one asks for a route, but a chaser whose target moved in plain sight moves only its last waypoint. A walk is asked through a contract of `units`, which a capability reads and writes with no dependency on navigation, never navigation's own state: `Destination`, the goal, which an order, a cast that walks into range, a train's rally, a build and a gather set, and navigation pursues and drops on arrival; and `ArrivedShort`, the goal a walker arrived short of, which navigation derives from its route as the route changes, so `Destination::walk_to` sends no walker again to a goal it arrived short of, and `Destination::gives_up` tells a walk that ended short of its target, or a unit that cannot walk. `Destination`'s state name is `units.destination`; its route's check covers the cells it walks over, and the client predicts it as it predicts the route. The unit walks from waypoint to waypoint, the rest of its step carried past each. When the static bodies change, a route they block is planned again. A unit at the end of a route that ends short of its goal has arrived short: it drops its destination and keeps the route, so the same goal plans nothing; when a static body goes, it walks again and its route is planned again.
- **Short route:** a walker whose next stretch would overlap a unit of its layer that stands plans A* in a window of 17 × 17 cells round itself, those units as blockers, to the last waypoint in the window or where its way leaves it; a window with no cell to stand in plans nothing. Walkers steer in stable-id order with the work the long routes left; one that finds none keeps its route and steers in a later tick. One kept back for 150 ms treats those it touches as half their reach to its left, so it goes round on its right and two that meet head on pass. A predicting client counts the units it holds that stand; one that started to walk since costs a correction.
- **Spatial indexes:** a grid of buckets for moving bodies and a static index for units that cannot walk, as 0 A.D. keeps them, and the index that target, flight, area and aura queries search, each keep their bodies in levels by size ([Wide bodies](../13-bodies-uses-shop-gravity.md#wide-bodies), D3).

A client plans its own units' routes on the same grid, so it predicts them with no correction. Crowds add flow fields and ORCA later.

### Groups

A move order to many units moves them as a group ([Orders](control.md#orders)): the units of the order its player controls that walk, live and do not reset; an order that leaves one moves it alone. The group's box is the smallest axis-aligned rectangle that holds their positions, its centre the box's midpoint rounded toward negative infinity. When the goal lies outside the box, each unit's goal is the order's goal plus its offset from the centre, taken into the bounds; inside or on its edge, every unit's goal is the order's, as StarCraft II keeps a selection's formation only for a move away from it.

Its units plan together when the first comes to be planned: one long route for each layer, for the widest walker of that layer among them, from the unit of that walker nearest the centre of their positions, the lower stable id on a tie, to the order's goal; a narrower walker has every cell the widest has open. Each unit joins the route at the last waypoint its own straight line reaches by its walker's smoothing test, tested from the goal back, and ends at its own goal when its line from the waypoint before reaches it, else at the order's goal. A unit that reaches no waypoint plans its own route. The line tests count in the tick's work limit; the first unit's run whatever the limit, so the group moves on each tick, and the units the limit leaves wait and plan together again. So a group costs one search a layer, not one a unit.

A spawn group walks as one: units of one spawn group on their path that wait for a route to one goal share it by the same rule. A unit that left its path plans alone.

### Waypoint paths

A map can hold paths of waypoints, as a MOBA's lanes or a patrol. A unit that walks a path goes along it in its spawn's direction, from the start or the end, while it has no other order. It reaches a waypoint once the waypoint is within its body, and walks a route to each round the structures between; the load refuses a placed unit that cannot walk whose body comes closer to a waypoint than the widest walker's radius.

### Forced movement

A dash, a knock back and a teleport move a walking unit by effects and calls ([Effects](actions.md#effects)), not by its step. Dashes and knock backs move in Move, after units walk, by stable id:

- **Dash:** `ctx.dash(unit, to, speed)` or `move = { to, speed }` moves it on the ground plane, at its height, a fixed step each tick of `speed` over the tick rate, rounded once, to a point, or to a unit, whose place of each tick it follows until their bodies touch, as Lee Sin follows his mark; at a box, the box's nearest point. A dash at a unit gone or dead ends where it is.
- **Knock back:** `ctx.knock_back(unit, from, distance, ms)` or `move = { from, distance, ms }` moves it `distance` straight away from `from` over `ms`, rounded up to ticks, each tick an equal share of the way left, so the last ends on its end; a unit on `from` goes along x.
- **Teleport:** `ctx.teleport(unit, pos)` puts it at once at `pos`, taken into the bounds, or, where its walker may not stand, at the centre of the nearest cell it may, ties to the lower number; on a grid with no such cell it stays.

A dash or knock back passes through units, as in League of Legends and Dota 2: steering and collision skip its unit. It ends before the first step a static body of its layer blocks, by a route line's exact test, and at the bounds; collision parts it from the units it ends on. A knock back also ends before a step into a cell the walls block, the cell it starts in not counted, so a unit pushed into a wall's margin can still be knocked away. A dash crosses walls, as League of Legends' dashes do, and where it ends in a cell its walker may not stand in, it goes to the nearest one, as a teleport does. While moved, a unit takes no step, starts no action, and its windup or channel is interrupted; its order waits, and it asks its route again where it ends. A new forced move replaces the one under way, and a teleport ends it. A teleport disjoints the homing projectiles that target the unit; a dash and a knock back do not, as Blink and Force Staff in Dota 2. A dash an instant action starts in its `on_resolve` is that action's delivery: its `on_end` runs in Hit of the tick the dash ends, with a hit whose target is the dash's unit target or `()`, whose place is where it ended, distance the way it went, and direction its last step's. A dash that is replaced, ended by a teleport, or whose unit dies delivers nothing. A forced move is state, and a client continues one it learned of.

### Map checks

The load checks, for the widest walker of each layer among the map's placed units, that every waypoint of a path is in a reachable set of the one before it, and that every marker of a spawn tag of the mode's, and every waypoint, is a place it may stand. Placed static bodies may overlap the walls and each other, as they block alike; only a box past the bounds is refused ([Units that drive](../17-units-that-drive.md#decisions), D5). It uses the regions the match uses, so a map that loads can be walked at its start; a structure spawned later may still close a way, and a route then ends at the nearest reachable cell.

### Collision

In Collide, each pair of living bodies on one layer that overlap as the stage starts parts along the line between them, pair by pair in stable-id order, by exact fixed-point steps, each coordinate rounded away from zero, until they touch or stand apart; a pair that overlaps only after those pushes parts next tick. A walker that runs into a unit that stands takes the whole overlap, so a standing unit body blocks; two that both walk or both stand share it; one that cannot walk, as a tower, leaves all of it to the other. A body that overlaps a box, which never walks, moves straight away from the box's nearest point, or out through the nearest edge, the first in corner order on a tie, rounded away from the box. A new box moves the walkers it overlaps to open cells as it spawns ([Placement](production.md#placement)). Gatherers in their loop do not part ([Gathering](production.md#gathering)). Touching is not overlap; two on one spot part along x. A predicting client parts its own units only from held units that cannot walk; a contact with any other comes from the server as a correction.

## State and derived

- **State:** each unit's route, progress (the ticks it has been kept back), its path and direction, its spawn group; and, kept in `units` for the other capabilities, each unit's destination and the forced move under way.
- **Derived:** the pathing grids, regions and static indexes, from the map and the units that cannot walk; a unit's step a tick, from its stats; and `ArrivedShort`, kept in `units`, from its route.

## Script API

`ctx.map` with `paths` and `markers(tag)`; `ctx.spawn_group(team, path, from, types)`, `from` a `PathEnd` ([Engine enums](../08-script-api.md#engine-enums)); `ctx.dash`, `ctx.knock_back`, `ctx.teleport`, each failing for a unit with no walker, a speed of no step a tick, a distance not above 0 or past the world's bound, or a time not above 0; `unit.path`, `unit.pos`, `unit.radius` (`()` for a box).

## Network

Positions go to everyone who sees the unit. A client predicts its own units' routes, steps and collisions with the units that cannot walk, and continues a forced move it learned of. The path a unit walks and its place on it go to its owner, of the kind `prediction` ([Replication audiences](../14-replication-audiences.md), D12).

## Cost

Long and short routes cost at most as many units of work a tick as the grid has cells. A unit takes about 20 ns on one core of a Ryzen 7 6800U, `integration/route_planner/walled`, so a tick's limit takes about 1.2 ms on a grid of 61,504 half-meter cells, 3.7 % of a tick at 30 Hz: the limit bounds the worst tick at a small share. Each walker costs a step, a steering test and its bucket's contacts a tick; a change of static bodies the chunks it touches; the Move stage's worst is `integration/server_stage/worst_3v3_move`.

A box's tests run in `i128` steps, and most end early, at the box's extent or at the bounds of an edge's line, which take no direction: `atomic/body_box/reach`, `reach_box` and `overlap`, and `integration/collision/boxes`. A group plans its shared route at about a tenth of the cost of its units one by one, `integration/route_planner/group` against `group_alone`, which is why a group shares one route.

Collide, for 1,000 bodies, is `integration/collision/crowded`, in 40 m square, and `spread`, over 120 m square. A static body entering the pathing grid, `integration/pathing_grid/enter`, costs what grows with the walker sizes and the chunks a body spans, not with the map.

## Genres

A MOBA's lanes and body blocking; an RTS's groups, air and ground layers and, later, flow fields; an MMO's navmesh for monsters; a shooter's and a battle royale's bots on the navmesh.
