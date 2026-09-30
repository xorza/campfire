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

## Waypoint paths

A map can hold paths of waypoints, such as a MOBA's lanes. A unit that walks a path goes along it forward or backward, as its spawn sets, while it has no other order: a MOBA's two sides each walk a lane from their own end. It has reached a waypoint once the waypoint is within its body, as walkers that push each other never stand on one point. A unit that does not walk never moves, so a map keeps its structures beside its paths, clear of the widest walker: the mode's load refuses a structure whose body comes closer to a path than that walker's radius.

## Collision

A unit type may declare a body: a circle of its radius on the ground plane. In the Collide stage, after Move, each pair of living bodies that overlap parts along the line between them, in stable-id order, by exact fixed-point steps: two that walk share the overlap, and a unit that does not walk leaves all of it to the other. Touching is not overlap; two on one spot part along x. A predicting client parts its own units from the held units that do not walk, whose places never go stale; a push by a held unit that walks comes from the server as a correction.
