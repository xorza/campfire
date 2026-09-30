# Navigation

## Space

Every map has bounds, a closed rectangle on the ground plane, and no unit is ever outside them: move orders clamp to them, the core clamps every unit that moved after Move and Collide, and a spawn outside them fails. Positions are 3D. On a grid map gameplay is on the ground plane: collision, pathfinding, ranges and vision use x and z, and y comes from the map's height data. High ground is a vision rule in the grid, not a height test. A `character` moves in full 3D against level geometry.

## Backends

| Backend | Does |
| --- | --- |
| Grid | A* for the long route |
| Navmesh | Routes over level geometry, for bots and units that take orders in 3D levels |
| Flow field | One field for many units to the same goal, for RTS groups |
| Local steering | Follows the route, avoids units and allows body blocking; shared by every backend, in fixed-point with a fixed unit order |

Queries take world positions, a route is a list of waypoints, and obstacles are shapes, so steering works with any backend. Buildings and doors change the map while it runs; each backend updates its obstacles from them.

## Movement

Three layers, as in Dota 2: a long route around what never moves, a short route around units in the way, and collision as the last guard. All are exact fixed point, in stable-id order, with a fixed work limit a tick.

- **Pathing grid:** the bounds in `[navigation] cell` cells, blocked for each walker radius near a unit that cannot walk. It changes when such a unit dies or spawns.
- **Long route:** A* for the unit's radius, eight neighbors with no corner cut, costs 10 and 14, ties by estimate then cell, then line-of-sight smoothing. An unreachable goal gives the nearest reachable cell. Routes wait in the order they were asked, then by stable id; a tick expands up to as many cells as the grid has, and the route that meets that limit finishes.
- **Short route:** a small local plan around units that stand, or that block the unit for a few ticks.
- **Collision:** the push-out below, over a grid of buckets for moving bodies and a static index for units that cannot walk, as 0 A.D. keeps them.

A client plans its own routes on the same grid. Crowds, as in an RTS, add flow fields and ORCA later.

## Waypoint paths

A map can hold paths of waypoints, such as a MOBA's lanes. A unit that walks a path goes along it forward or backward, as its spawn sets, while it has no other order: a MOBA's two sides each walk a lane from their own end. It has reached a waypoint once the waypoint is within its body, as walkers that push each other never stand on one point. A unit that does not walk never moves, so a map keeps its structures beside its paths, clear of the widest walker: the mode's load refuses a structure whose body comes closer to a path than that walker's radius.

## Collision

A unit type may declare a body: a circle of its radius on the ground plane. In the Collide stage, after Move, each pair of living bodies that overlap as the stage starts parts along the line between them, pair by pair in stable-id order, by exact fixed-point steps; a pair that only overlaps after those pushes parts in the next tick. A unit walking to a destination that runs into one that stands takes the whole overlap, so no unit shoves another aside: a standing unit body blocks. Two that both walk, or both stand, share it, and a unit that cannot walk, such as a tower, leaves all of it to the other. Touching is not overlap; two on one spot part along x. A predicting client parts its own units only from the held units that cannot walk, such as towers, which never move. Every other held unit is where the server last had it, behind the client's ticks, and may have started or stopped walking since, so a contact with it comes from the server as a correction.
