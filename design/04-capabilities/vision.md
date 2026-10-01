# Vision

What each team may see, and so what each client receives. Only what a team sees is sent to its clients.

## Backends

| Backend | Does |
| --- | --- |
| Grid fog of war | Each unit reveals the cells within its sight range, blocked by terrain; brush cells block sight from outside the brush |
| 3D occlusion | An enemy is sent only when visible or about to become visible; footsteps and gunshots go only to players within hearing range. On large terrain, far-field occlusion uses terrain height and distance, and full tests run only up close |
| Relevance | For large maps: what a client receives weighs distance, view direction and scope state; far units update less often (Lightyear bandwidth priority) |

## Grid fog of war

The grid divides the map's bounds into square cells of the map's cell size; a point on the max edge lies in the last cell, so every unit stands in a cell. In the Vision stage, the last stage of a tick, each living unit with a `sight_range` reveals to its team every cell whose center is within that range on the ground plane. A unit is seen by its own team always, and by each team that revealed the cell it stands in. The rule is the same for every unit: a structure too is seen only while in sight. Terrain and brush do not block sight yet.

What each team sees is state: it is hashed with the rest, and the queries of the next tick read it. After each tick the server replicates each unit to the clients whose team sees it, from the tick it comes into sight until the tick it leaves sight, when the client despawns it. A unit hidden in the tick it spawns never reaches that client. A sight range should exceed the reach it chooses targets within by half a cell's diagonal, so a unit sees every target it may take.

## Dynamic blockers

Smoke, closed doors and walls built during a match block sight while they stand, in every backend.

## Stealth

A `stealthed` unit is visible only to its team and to an enemy team one of whose units is in the `true_sight` state and sees the unit's cell within its sight range, as Dota 2's true sight reveals invisible units ([Stats](stats.md#states)). True sight comes from a unit type, as a tower's, or from a modifier, as a ward's or a consumable's. Wards are units with a sight range and no collision.
