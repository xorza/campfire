# MMO kit

Status: later. A persistent world on one server; the mode never calls `ctx.end`.

## World state

- Characters, inventories, guild holdings and world state persist through core saves at tick boundaries. Scripts never write to a database.
- Character load on login, admin commands, purchases and calendar events ("Saturday 20:00: siege starts") are recorded inputs.
- Content upgrades apply at restart; script state migrations convert saved state.

## Scale

- Low tick rate (10–20 Hz), strict per-client relevance.
- Dormant regions: areas with no players nearby stop thinking. Dormancy depends on sim state (player proximity), never the wall clock.
- Assets stream by region.

## Systems

| In the sim (game state) | Outside the sim (server services) |
| --- | --- |
| Inventory, trade, parties, guild ownership, quests | Chat, mail, friend lists |

Quests and NPC dialogue are mostly scripts; the kit provides inventory and quest-state helpers.

## Payments

Time-based and per-event payments; no wagers.
