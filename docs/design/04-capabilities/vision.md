# Vision

## Mechanism

What each vision group may see, and so what each client receives. Only what a client's group sees is sent to it. Friendly teams form a vision group unless their relation turns vision off ([Relations](00-overview.md#relations)); a unit hidden by its tags is seen only through detection.

## Data

A unit type's `vision = { sight_range }`, in meters, not negative. The map's `[grid]`, with `cell` in meters, positive, the cells the grid fog of war marks; a mode with the grid backend needs it; and `[[grid.brush]]`, each a simple polygon of `points`, as a wall's ([Navigation](navigation.md#data)). The mode's tags with `hidden` and `detects` ([Tags](stats.md#tags)).

## Rules

### Backends

| Backend | Does |
| --- | --- |
| Grid fog of war | Each unit reveals the cells within its sight range, blocked by terrain; brush cells block sight from outside the brush; the one the release runs |
| 3D occlusion | A unit is sent only when visible or about to become visible; footsteps and gunshots go only to players within hearing range. On large terrain, far-field occlusion uses terrain height and distance, and full tests run only up close |
| Relevance | For large maps and many players: each client receives what a spatial grid finds near its view, weighed by distance, view direction and scope state; far units update less often, as Fortnite's replication graph does for 100 players |

### Grid fog of war

The grid divides the map's bounds into square cells of the map's cell size; a point on the max edge lies in the last cell, so every unit stands in a cell. In the Vision stage, the last stage of a tick, each living unit with a `sight_range` reveals to its group every cell whose center is within that range on the ground plane, on either metric: the cells are squares of the ground plane, with no height. A unit is seen by its own group always, from the moment it spawns, before its first Vision stage, and by each group that revealed a cell it stands in. A unit with a box body stands in every cell whose square its box overlaps, by the overlap rule of shapes, touching not counted ([Space and map](00-overview.md#space-and-map)), so a building half in sight is seen. A box never moves, so its cells are found once, as it spawns, and kept as one run of cells for each row; the stage tests each run against a group's bitmap a word at a time. The rule is the same for every unit: a structure, a projectile and an item on the ground too are seen only while in sight.

- **Brush.** A cell whose center lies inside a brush polygon, or on its edge, is that brush's, the first the map lists. A unit reveals a brush's cells only while it stands in that brush, as League of Legends' brush hides who stands in it from everyone outside; it sees out of the brush as from anywhere. A ward in a brush sees into it.
- **Reveals.** `ctx.reveal(pos, radius, ms)` reveals to the acting unit's group the cells whose centers lie within `radius` of `pos`, brush cells included, from that tick's Vision stage for `ms`, as Farsight and Snow Owl do; it detects no hidden unit. A call with no acting unit, a negative radius, or a time of zero fails.
- **Walls** block no sight yet: a unit sees over them, as the 3v3's lanes need no more. The grid keeps one bitmap for each group, and a detection bitmap for each group that has a detector, and each tick clears only the words the tick before set, so the stage costs what the units see, not the map's size times the groups. A map with a vision grid holds at most 64 teams, and so at most 64 groups; the load refuses more.

What each group sees is state, and so are the reveals under way: they are hashed with the rest, and the queries of the next tick read them. After each tick the server replicates each unit to the clients whose group sees it, from the tick it comes into sight until the tick it leaves sight, when the client despawns it. A unit hidden in the tick it spawns never reaches that client. The server keeps this as Lightyear's rooms, one for each team: a unit stands in the rooms of the teams that see it, every team's in a match with no vision, and a seated link in its team's, so Lightyear sends a link what its team sees whenever it sits, and a link with no seat nothing. A sight range should exceed the reach it chooses targets within by half a cell's diagonal, so a unit sees every target it may take.

### Hidden units

A unit with a `hidden` tag is seen only by its own group and by a group one of whose units has a `detects` tag and sees a cell the unit stands in within its sight range, as Dota 2's true sight reveals invisible units and StarCraft II's detectors reveal cloaked ones. Detection comes from a unit type's tag, as a tower's, or from a modifier, as a ward's or a consumable's. Wards are units with a sight range and no collision.

### Senses

What one unit perceives, for AI that sneaks and is sneaked on, as Skyrim's guards notice a thief. The engine gives facts, each exact and the same everywhere; the mode decides what they mean, in its scripts or a scripted system:

- **Sight:** `unit.sees(other)`, whether `other` is within the unit's sight range, within its view cone (the type's `vision = { cone_deg }`), and not behind level geometry or a blocker on the line between them; and the light at `other`'s place, from the map's light and the game clock.
- **Hearing:** an action's or an effect's `noise`, heard by the units within its radius in the tick it is made: `on_heard(ctx, unit, source, pos)` runs in the unit's AI, in the Think stage of the next tick.

A detection meter, a sneak skill against a perception stat, and a guard's search are the mode's rules on those facts.

### Dynamic blockers

Smoke, closed doors and walls built during a match block sight while they stand, in every backend.

## State and derived

- **State:** which groups see each unit, from the last Vision stage; the reveals under way, each its team, place, radius and last tick; its team's group sees it, as the groups follow the relations.
- **Derived:** the vision groups, from the relations; each cell's brush, from the map; the revealed cells of each group, each tick.

## Script API

`ctx.find_visible`, `ctx.nearest_visible`, `ctx.reveal(pos, radius, ms)`; `unit.can_see(other)`.

## Network

Each unit goes to the clients whose group sees it, and leaves them when no longer seen. A client draws what it holds; hidden information never reaches it.

## Cost

The grid fog costs each living unit with sight the cells within its range, and each unit a test of its cell against each group's bitmaps. Relevance costs each client the grid cells near its view.

The Vision stage's grid fog, for 1000 units of two teams with a 10 m sight on 1 m cells, on one core of a Ryzen 7 6800U, reveals their cells and finds who sees each in 0.17 ms when every unit moved since the tick before, however close they stand: 0.5 % of a tick at 30 Hz, `fog/sight`; and in 0.07 ms when none did, `fog/still`. A sight's rows go four at a time in `math`'s lanes, each with an exact integer root ([Libraries](../02-engine-core.md#libraries)). A sight covers the same runs of cells from the same place with the same range, so a unit that stood still since the tick before takes its runs again, from a cache of each unit's runs of the tick before: in the 3v3 about two sights in three.

## Genres

A MOBA's and an RTS's grid fog, with stealth or cloak and detection; a shooter's occlusion and smoke; a battle royale's and an MMO's relevance over a large map.
