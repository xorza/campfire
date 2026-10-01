# Vision

## Mechanism

What each vision group may see, and so what each client receives. Only what a client's group sees is sent to it. Friendly teams form a vision group unless their relation turns vision off ([Relations](00-overview.md#relations)); a unit hidden by its tags is seen only through detection.

## Data

A unit type's `vision = { sight_range }`, in meters, not negative. The map's `[grid]`, with `cell` in meters, positive, the cells the grid fog of war marks; a mode with the grid backend needs it. The mode's tags with `hidden` and `detects` ([Tags](stats.md#tags)).

## Rules

### Backends

| Backend | Does |
| --- | --- |
| Grid fog of war | Each unit reveals the cells within its sight range, blocked by terrain; brush cells block sight from outside the brush; the one the release runs |
| 3D occlusion | A unit is sent only when visible or about to become visible; footsteps and gunshots go only to players within hearing range. On large terrain, far-field occlusion uses terrain height and distance, and full tests run only up close |
| Relevance | For large maps and many players: each client receives what a spatial grid finds near its view, weighed by distance, view direction and scope state; far units update less often, as Fortnite's replication graph does for 100 players |

### Grid fog of war

The grid divides the map's bounds into square cells of the map's cell size; a point on the max edge lies in the last cell, so every unit stands in a cell. In the Vision stage, the last stage of a tick, each living unit with a `sight_range` reveals to its group every cell whose center is within that range in the map's metric. A unit is seen by its own group always, and by each group that revealed the cell it stands in. The rule is the same for every unit: a structure, a projectile and an item on the ground too are seen only while in sight. Terrain and brush do not block sight yet. The grid keeps one bitmap for each group, at most 64 groups.

What each group sees is state: it is hashed with the rest, and the queries of the next tick read it. After each tick the server replicates each unit to the clients whose group sees it, from the tick it comes into sight until the tick it leaves sight, when the client despawns it. A unit hidden in the tick it spawns never reaches that client. A sight range should exceed the reach it chooses targets within by half a cell's diagonal, so a unit sees every target it may take.

### Hidden units

A unit with a `hidden` tag is seen only by its own group and by a group one of whose units has a `detects` tag and sees the unit's cell within its sight range, as Dota 2's true sight reveals invisible units and StarCraft II's detectors reveal cloaked ones. Detection comes from a unit type's tag, as a tower's, or from a modifier, as a ward's or a consumable's. Wards are units with a sight range and no collision.

### Senses

What one unit perceives, for AI that sneaks and is sneaked on, as Skyrim's guards notice a thief. The engine gives facts, each exact and the same everywhere; the mode decides what they mean, in its scripts or a scripted system:

- **Sight:** `unit.sees(other)`, whether `other` is within the unit's sight range, within its view cone (the type's `vision = { cone_deg }`), and not behind level geometry or a blocker on the line between them; and the light at `other`'s place, from the map's light and the game clock.
- **Hearing:** an action's or an effect's `noise`, heard by the units within its radius in the tick it is made: `on_heard(ctx, unit, source, pos)` runs in the unit's AI, in the Think stage of the next tick.

A detection meter, a sneak skill against a perception stat, and a guard's search are the mode's rules on those facts.

### Dynamic blockers

Smoke, closed doors and walls built during a match block sight while they stand, in every backend.

## State and derived

- **State:** which groups see each unit, from the last Vision stage.
- **Derived:** the vision groups, from the relations; the revealed cells of each group, each tick.

## Script API

`ctx.find_visible`, `ctx.nearest_visible`, `ctx.reveal(pos, radius, ms)`; `unit.can_see(other)`.

## Network

Each unit goes to the clients whose group sees it, and leaves them when no longer seen. A client draws what it holds; hidden information never reaches it.

## Cost

The grid fog costs each living unit with sight the cells within its range, and each unit a test of its cell against each group's bitmaps. Relevance costs each client the grid cells near its view.

## Genres

A MOBA's and an RTS's grid fog, with stealth or cloak and detection; a shooter's occlusion and smoke; a battle royale's and an MMO's relevance over a large map.
