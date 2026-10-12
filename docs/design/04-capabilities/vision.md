# Vision

## Mechanism

What each vision group may see, and so which units each client receives. Friendly teams form a vision group unless their relation turns vision off ([Relations](00-overview.md#relations)); a unit hidden by its tags is seen only through detection.

## Data

A unit type's `vision = { sight_range }`, in meters, not negative. The map's `[grid]`: `cell` in meters, positive, which a mode with the grid backend needs, and `[[grid.brush]]`, each a simple polygon of `points`, as a wall's ([Navigation](navigation.md#data)). The mode's tags with `hidden` and `detects` ([Tags](stats.md#tags)).

## Rules

### Backends

| Backend | Does |
| --- | --- |
| Grid fog of war | Each unit reveals the cells within its sight range; brush cells block sight from outside the brush; the one the release runs |
| 3D occlusion | Later: a unit is sent only when visible or about to be; sounds only within hearing range; far-field occlusion on large terrain |
| Relevance | Later: for large maps and many players, each client receives what a spatial grid finds near its view, weighed by distance and direction, far units less often, as Fortnite's replication graph does |

### Grid fog of war

The grid divides the bounds into square cells, a point on the max edge in the last cell. In Vision, the last stage, each living unit with a `sight_range` reveals to its group every cell whose centre is within that range on the ground plane, on either metric. A unit is seen by its own group always, from its spawn, and by each group that revealed a cell it stands in. A box stands in every cell its box overlaps, touching not counted, so a building half in sight is seen; its cells are found once, as it spawns, and kept as runs by row, which the stage tests against a group's bitmap a word at a time. The rule is the same for every unit: a structure, a projectile and an item on the ground are seen only while in sight.

- **Brush.** A cell whose centre lies in or on a brush polygon is that brush's, the first the map lists. A unit reveals a brush's cells only while it stands in that brush, as League of Legends' brush hides who stands in it; it sees out of the brush as from anywhere, and a ward in a brush sees into it.
- **Reveals.** `ctx.reveal(pos, radius, ms)` reveals to the acting unit's group the cells within `radius` of `pos`, brush cells included, from that tick's Vision for `ms`, as Farsight and Snow Owl do; it detects no hidden unit. A call with no acting unit, a negative radius or a zero time fails.
- **Walls** block no sight yet. The grid keeps a bitmap for each group, and a detection bitmap for each group with a detector, and each tick clears only the words the tick before set, so the stage costs what units see. A map with a vision grid holds at most 64 teams.

What each group sees is state, as are the reveals under way, and the next tick's queries read them. After each tick the server replicates each unit to the clients whose group sees it, from the tick it comes into sight until it leaves, when the client despawns it; a unit hidden in its spawn tick never reaches that client. The server keeps Lightyear's rooms, one for each team: a unit stands in the rooms of the teams that see it, every team's in a match with no vision, and a seated link in its team's room; a spectator's link stands in the rooms its fog gives ([Replication audiences](../14-replication-audiences.md), D15). A sight range should exceed the reach a unit chooses targets within by half a cell's diagonal, so it sees every target it may take.

### Hidden units

A unit with a `hidden` tag is seen only by its own group and by a group with a `detects` unit that sees a cell it stands in within its sight, as Dota 2's true sight and StarCraft II's detectors. Detection comes from a type's tag, as a tower's, or a modifier, as a ward's or a consumable's. Wards are units with sight and no collision.

### Senses

Planned. What one unit perceives, for AI that sneaks and is sneaked on, as Skyrim's guards notice a thief; the engine gives exact facts and the mode decides what they mean:

- **Sight:** `unit.sees(other)`, whether `other` is within the unit's sight range, its view cone (`vision = { cone_deg }`), and not behind level geometry or a blocker; and the light at `other`'s place, from the map's light and the game clock.
- **Hearing:** a `noise`, heard by the units within its radius in its tick: `on_heard(ctx, unit, source, pos)` runs in the unit's AI in the next Think.

### Dynamic blockers

Planned. Smoke, closed doors and walls built during a match block sight while they stand, in every backend.

## State and derived

- **State:** which groups see each unit, from the last Vision; the reveals under way, each its team, place, radius and last tick.
- **Derived:** the vision groups, from the relations; each cell's brush; each group's revealed cells, each tick.

## Script API

`ctx.find_visible`, `ctx.nearest_visible`, `ctx.reveal(pos, radius, ms)`; `unit.can_see(other)`.

## Network

Each unit goes to the clients whose group sees it, and leaves them when no longer seen; hidden information never reaches a client. What each group sees and each unit's sight stay the server's: a predicting client reveals nothing, and takes each unit it holds as seen ([Replication audiences](../14-replication-audiences.md), D12).

## Cost

The grid fog costs each living unit with sight the cells within its range, and each unit a test of its cells against each group's bitmaps. For 1,000 units of two teams with 10 m sight on 1 m cells, on one core of a Ryzen 7 6800U, the stage takes 0.17 ms when every unit moved, `integration/fog/sight`, 0.5 % of a tick at 30 Hz, and 0.07 ms when none did, `integration/fog/still`. A sight's rows go four at a time in `math`'s lanes, each with an exact integer root, and a unit that stood still reuses its runs of the tick before, about two sights in three in the 3v3.

## Genres

A MOBA's and an RTS's grid fog, with stealth or cloak and detection; a shooter's occlusion and smoke; a battle royale's and an MMO's relevance.
