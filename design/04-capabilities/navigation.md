# Navigation

## Mechanism

How units move: on layers, by routes round what blocks them, along waypoint paths, by forced movement, and apart from each other by collision. The space it works in, the map's metric, layers and bounds, is the core's ([Space and map](00-overview.md#space-and-map)).

## Data

- **Mode:** `[navigation] layers = ["ground", "air"]`, the first the default; a mode that names none has one layer, with no name.
- **Map:** `[navigation]` with `cell`, in meters, positive: whole cells over the bounds, at most 2²² cells, the cells routes are planned on; for each layer, the cells its map blocks, none for a layer such as `air`. `[[paths]]`, each a `name` and `points`.
- **Unit type:** `collision = { radius, layer }`: a body, a circle of its radius on the ground plane, up to 64 m, on its layer, the mode's first when it names none; a type without it collides with nothing, and moves on the first layer. A unit that can walk has the `move_speed` stat.

## Rules

### Backends

| Backend | Does |
| --- | --- |
| Grid | A* for the long route and the short route; the one the release runs |
| Navmesh | Routes over level geometry, for bots and units that take orders in 3D levels; later |
| Flow field | One field for many units to the same goal, for RTS groups; later |
| Local steering | Follows the route, goes round units that stand or block, and allows body blocking; shared by every backend, in fixed-point with a fixed unit order |

Queries take world positions, a route is a list of waypoints, and obstacles are shapes, so steering works with any backend. Buildings and doors change the map while it runs; each backend updates its obstacles from them.

### Layers

Each layer has its own pathing grid, its own static bodies and its own collision: a unit's route goes round the map's blocked cells of its layer and the bodies on it that cannot walk, and its body touches only bodies on its layer. A unit's layer is a tag of it, so a weapon's filter selects air or ground (`enemies:air`). A shooter or a MOBA has one layer.

### Movement

Three kinds of avoidance, as in Dota 2: a long route around what never moves, a short route around units in the way, and collision as the last guard. All are exact fixed point, in stable-id order, with a fixed work limit a tick. A unit whose tags block `move` stands, and keeps its destination and route ([Tags](stats.md#tags)); to the units round it, it is a unit that stands.

- **Pathing grid:** the bounds in `[navigation] cell` cells, blocked for each kind of walker, a layer and a radius, near the map's blocked cells of its layer and near a unit of its layer that cannot walk. It changes when such a unit dies or spawns.
- **Regions:** for each kind of walker, the cells a walker can go between, as 0 A.D.'s hierarchical pathfinder keeps them: the grid in chunks of 64 × 64 cells, each chunk's open cells split into the regions that touch along a side, regions of chunks side by side joined where their cells touch, and the joined regions numbered as reachable sets. Touching along a side is exact for the moves A* takes: a diagonal step needs both cells beside it open, so it never joins what side steps do not. A change of the static bodies builds again only the chunks whose cells changed, then numbers the reachable sets again over the regions, not the cells. A cell that is blocked, as a start or a goal the walker may stand on can be, belongs to the reachable sets of its open side neighbors.
- **Long route:** A* for the unit's kind of walker, eight neighbors with no corner cut, costs 10 and 14, ties by estimate then cell, then smoothing that keeps a waypoint only where the straight line on would overlap a static body, tested exactly. A goal the unit cannot stand on gives the nearest open cell; one in no reachable set of the start's gives the nearest cell in one, as 0 A.D.'s `MakeGoalReachable` does, found before the search, so no search ever spreads over a whole region to learn that it fails. Each region keeps the box of its cells; the start's regions are taken in order of how near their box comes to the goal, each region's cells scanned for the nearest, until a box comes no nearer than the best cell found; ties go to the lower cell number. The nearest open cell is found by rings of cells round the goal's, each ring by its edge, out to the edge of the grid at most, so a search visits each cell once at most. Routes wait in the order they were first asked, then by stable id: a new goal for a route that waits keeps its place. Long and short routes share one limit of work a tick, as many units as the grid has cells: a cell expanded, a cell scanned for the nearest cell, a straight line tested against the bodies. Long routes go first; the plan that meets the limit finishes, and the rest wait for a later tick.
- **Walking a route:** a move order, a chase and a path walker set the destination; a new one asks for a route, except that a chaser whose target moved in plain sight moves only its last waypoint. The unit walks from waypoint to waypoint, the rest of its step carried past each. When the static bodies change, a route they now block is planned again; a route they now leave shorter is kept.
- **Short route:** a walker whose next stretch would overlap a unit of its layer that stands plans A* in a window of 17 × 17 cells around itself, those units as blockers, to the last waypoint in the window or where its way leaves it. Its nearest open cell is searched in the window alone; a window with no cell to stand in plans nothing, and the walker keeps its route and destination. Walkers steer in stable-id order with the work the long routes left; one that finds none keeps its route, yields as collision does, and steers in a later tick. One that walkers keep back for 150 ms marks those it touches too, each as if half their reach to its left, so it goes round on its right and two that meet head on pass. A predicting client counts the units it holds that stand; one that started to walk since the server sent it costs a correction.
- **Collision:** the push-out below, over a grid of buckets for moving bodies and a static index for units that cannot walk, as 0 A.D. keeps them.

A client plans its own units' routes on the same grid, so it predicts them with no correction. Crowds, as in an RTS, add flow fields and ORCA later.

### Waypoint paths

A map can hold paths of waypoints, such as a MOBA's lanes or a patrol route. A unit that walks a path goes along it in the direction its spawn names, from the start or from the end, while it has no other order: a MOBA's two sides each walk a lane from their own end, and a third team may walk it either way. It has reached a waypoint once the waypoint is within its body, as walkers that push each other never stand on one point. It walks a route to each waypoint, round the structures between, but must stand on the waypoint to reach it: the mode's load refuses a placed unit that cannot walk whose body comes closer to a waypoint than the widest walker's radius.

### Forced movement

A dash, a knock back and a teleport move a unit through effects ([Effects](actions.md#effects)), not by its step: a dash or a knock back over the ground plane at its own speed, through the clearance the static bodies leave, a teleport at once. Each ends at the nearest place the unit may stand, and the unit plans its route again after. A teleport, and a move longer than an engine constant, disjoint the homing projectiles that target the unit. A dash ends with its action's `on_end`.

### Map checks

The mode's load checks, for the widest walker of each layer and the map's placed units, that every waypoint of a path is in a reachable set of the one before it, and that every spawn marker and waypoint is a place the widest walker may stand. A narrower walker has every cell the widest has open, so the widest is enough. The check uses the regions the match uses, so a map that loads is one the match can walk at its start; a structure a script spawns later may still close a way, and the route then ends at the nearest reachable cell.

### Collision

In the Collide stage, after Move, each pair of living bodies on one layer that overlap as the stage starts parts along the line between them, pair by pair in stable-id order, by exact fixed-point steps; a pair that only overlaps after those pushes parts in the next tick. A unit walking to a destination that runs into one that stands takes the whole overlap, so no unit shoves another aside: a standing unit body blocks. Two that both walk, or both stand, share it, and a unit that cannot walk, such as a tower, leaves all of it to the other. Touching is not overlap; two on one spot part along x. After Move and Collide, the core clamps every unit that moved into the bounds. A predicting client parts its own units only from the held units that cannot walk, such as towers, which never move. Every other held unit is where the server last had it, behind the client's ticks, and may have started or stopped walking since, so a contact with it comes from the server as a correction.

## State and derived

- **State:** each unit's destination, route, progress (the ticks it has been kept back), the path it walks and its direction; forced movement in progress.
- **Derived:** the pathing grids, regions and static indexes, from the map and the units that cannot walk; a unit's step a tick, from its stats.

## Script API

`ctx.map` with `paths` and `markers(tag)`; `ctx.spawn_group(team, path, from, types)`, `from` the path's `"start"` or `"end"`; `ctx.dash`, `ctx.knock_back`, `ctx.teleport`; `unit.path`, `unit.pos`, `unit.radius`.

## Network

Positions go to everyone who sees the unit. A client predicts its own units' routes, steps and collisions with the units that cannot walk.

## Cost

Long and short routes cost up to as many units of work a tick as the grid has cells, in total, past which only the plan that meets the limit runs. Each walking unit costs a step, a steering test and its bucket's contacts a tick. A change of static bodies costs the chunks it touches.

## Genres

A MOBA's lanes and body blocking; an RTS's groups, air and ground layers and, later, flow fields; an MMO's navmesh for monsters; a shooter's and a battle royale's bots on the navmesh.
