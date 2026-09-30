# Persistence

A persistent world on one server; its mode never calls `ctx.end`.

## World state

- Characters, their inventories (`items`) and levels (`progression`), guild holdings and world state persist through core saves at tick boundaries. Scripts never write to a database.
- Character load on login, admin commands, purchases and calendar events ("Saturday 20:00: siege starts") are recorded inputs.
- Content upgrades apply at restart; script state migrations convert saved state.

## Scale

A world of 1,000+ players is one session on one server. Dungeons, battlegrounds and cities are regions of that world.

- Dormant regions: areas with no players nearby stop thinking. Dormancy depends on sim state (player proximity), never the wall clock.
- Deterministic multithreading: regions that do not touch run their systems in parallel, and the result is the same on every machine.
- Relevance: each client receives only what is near and visible to it.
- Assets stream by region.

## Systems

| In the sim (game state) | Outside the sim (server services) |
| --- | --- |
| Trade, parties, guild ownership, quests | Chat, mail, friend lists |

Quests are mostly scripts; the capability provides quest-state helpers. Inventory is `items`, NPC dialogue is `interaction`.
