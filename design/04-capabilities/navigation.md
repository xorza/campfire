# Navigation

## Space

Positions are 3D. On a grid map gameplay is on the ground plane: collision, pathfinding, ranges and vision use x and z, and y comes from the map's height data. High ground is a vision rule in the grid, not a height test. A `character` moves in full 3D against level geometry.

## Backends

| Backend | Does |
| --- | --- |
| Grid | A* for the long route |
| Navmesh | Routes over level geometry, for bots and units that take orders in 3D levels |
| Local steering | Follows the route, avoids units and allows body blocking; shared by every backend, in fixed-point with a fixed unit order |

## Waypoint paths

A map can hold paths of waypoints, such as a MOBA's lanes. A unit that walks a path goes along it forward or backward, as its spawn sets, while it has no other order: a MOBA's two sides each walk a lane from their own end.
