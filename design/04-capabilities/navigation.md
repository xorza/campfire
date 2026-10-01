# Navigation

## Space

Every map has bounds, a closed rectangle on the ground plane, and no unit is ever outside them: move orders clamp to them, the core clamps every unit that moved after Move and Collide, and a spawn outside them fails. Positions are 3D. On a grid map gameplay is on the ground plane: collision, pathfinding, ranges and vision use x and z, and y comes from the map's height data. High ground is a vision rule in the grid, not a height test. A `character` moves in full 3D against level geometry.

## Backends

| Backend | Does |
| --- | --- |
| Grid | A* for the long route and the short route; the one the release runs |
| Navmesh | Routes over level geometry, for bots and units that take orders in 3D levels; later |
| Flow field | One field for many units to the same goal, for RTS groups; later |
| Local steering | Follows the route, goes round units that stand or block, and allows body blocking; shared by every backend, in fixed-point with a fixed unit order |

Queries take world positions, a route is a list of waypoints, and obstacles are shapes, so steering works with any backend. Buildings and doors change the map while it runs; each backend updates its obstacles from them.

## Movement

Three layers, as in Dota 2: a long route around what never moves, a short route around units in the way, and collision as the last guard. All are exact fixed point, in stable-id order, with a fixed work limit a tick.

- **Pathing grid:** the bounds in `[navigation] cell` cells, blocked for each walker radius near a unit that cannot walk. It changes when such a unit dies or spawns.
- **Regions:** for each walker radius, the cells a walker can go between, as 0 A.D.'s hierarchical pathfinder keeps them: the grid in chunks of 64 × 64 cells, each chunk's open cells split into the regions that touch along a side, regions of chunks side by side joined where their cells touch, and the joined regions numbered as reachable sets. Touching along a side is exact for the moves A* takes: a diagonal step needs both cells beside it open, so it never joins what side steps do not. A change of the static bodies builds again only the chunks whose cells changed, then numbers the reachable sets again over the regions, not the cells. A cell that is blocked, as a start or a goal the walker may stand on can be, belongs to the reachable sets of its open side neighbors.
- **Long route:** A* for the unit's radius, eight neighbors with no corner cut, costs 10 and 14, ties by estimate then cell, then smoothing that keeps a waypoint only where the straight line on would overlap a static body, tested exactly. A goal the unit cannot stand on gives the nearest open cell; one in no reachable set of the start's gives the nearest cell in one, as 0 A.D.'s `MakeGoalReachable` does, found before the search, so no search ever spreads over a whole region to learn that it fails. Each region keeps the box of its cells; the start's regions are taken in order of how near their box comes to the goal, each region's cells scanned for the nearest, until a box comes no nearer than the best cell found; ties go to the lower cell number. Routes wait in the order they were asked, then by stable id; a tick expands up to as many cells as the grid has, and the route that meets that limit finishes.
- **Walking a route:** a move order, a chase and a path walker set the destination; a new one asks for a route, except that a chaser whose target moved in plain sight moves only its last waypoint. The unit walks from waypoint to waypoint, the rest of its step carried past each. When the static bodies change, a route they now block is planned again; a route they now leave shorter is kept.
- **Short route:** a walker whose next stretch would overlap a unit that stands plans A* in a window of 17 × 17 cells around itself, those units as blockers, to the last waypoint in the window or where its way leaves it. One that walkers keep back for 150 ms marks those it touches too, each as if half their reach to its left, so it goes round on its right and two that meet head on pass. A predicting client counts the units it holds that stand; one that started to walk since the server sent it costs a correction.
- **Collision:** the push-out below, over a grid of buckets for moving bodies and a static index for units that cannot walk, as 0 A.D. keeps them.

A client plans its own units' routes on the same grid, so it predicts them with no correction. Crowds, as in an RTS, add flow fields and ORCA later.

## Map checks

The mode's load checks, for the widest walker and the map's structures, that every waypoint of a path is in a reachable set of the one before it, and that every avatar spawn, neutral spawn and waypoint is a place the widest walker may stand. A narrower walker has every cell the widest has open, so the widest is enough. The check uses the regions the match uses, so a map that loads is one the match can walk at its start; a structure a script spawns later may still close a way, and the route then ends at the nearest reachable cell.

## Waypoint paths

A map can hold paths of waypoints, such as a MOBA's lanes. A unit that walks a path goes along it forward or backward, as its spawn sets, while it has no other order: a MOBA's two sides each walk a lane from their own end. It has reached a waypoint once the waypoint is within its body, as walkers that push each other never stand on one point. It walks a route to each waypoint, round the structures between, but must stand on the waypoint to reach it: the mode's load refuses a structure whose body comes closer to a waypoint than the widest walker's radius.

## Collision

A unit type may declare a body: a circle of its radius on the ground plane. In the Collide stage, after Move, each pair of living bodies that overlap as the stage starts parts along the line between them, pair by pair in stable-id order, by exact fixed-point steps; a pair that only overlaps after those pushes parts in the next tick. A unit walking to a destination that runs into one that stands takes the whole overlap, so no unit shoves another aside: a standing unit body blocks. Two that both walk, or both stand, share it, and a unit that cannot walk, such as a tower, leaves all of it to the other. Touching is not overlap; two on one spot part along x. A predicting client parts its own units only from the held units that cannot walk, such as towers, which never move. Every other held unit is where the server last had it, behind the client's ticks, and may have started or stopped walking since, so a contact with it comes from the server as a correction.
