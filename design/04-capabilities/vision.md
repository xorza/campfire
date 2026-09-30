# Vision

What each team may see, and so what each client receives. Only what a team sees is sent to its clients.

## Backends

| Backend | Does |
| --- | --- |
| Grid fog of war | Each unit reveals the cells within its sight range, blocked by terrain; brush cells block sight from outside the brush |
| 3D occlusion | An enemy is sent only when visible or about to become visible; footsteps and gunshots go only to players within hearing range. On large terrain, far-field occlusion uses terrain height and distance, and full tests run only up close |
| Relevance | For large maps: what a client receives weighs distance, view direction and scope state; far units update less often (Lightyear bandwidth priority) |

## Stealth

A stealthed unit is visible only to its team and to enemies with true sight over it (vision wards, towers, consumables). Wards are units with a sight range and no collision.
