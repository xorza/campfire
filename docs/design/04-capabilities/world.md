# World

## Mechanism

Planned. A large world in one session: regions that sleep when no player is near, systems that run in parallel over regions that do not touch, and assets that stream by region. An MMO's persistent world, which never calls `ctx.end`, and a singleplayer open world are the same mechanism; what either keeps across a restart or a load is the core's checkpoints and saves ([Saves](../02-engine-core.md#saves)).

## Data

The map's regions: markers with the `region` tag, each a box. The mode's dormancy distance.

## Rules

- **Dormant regions:** a region with no player within the dormancy distance stops thinking: its units' AI, scripted systems and timers wait, by sim state alone. A region that wakes runs the mode's `on_wake(ctx, region, slept_ms)`, which places its units where the time slept would have taken them, as Skyrim places a town's people by their schedules ([Game clock](00-overview.md#game-clock)).
- **Deterministic multithreading:** regions that do not touch run their systems in parallel, with the same result on every machine.
- **Relevance:** each client receives what is near and visible to it ([Vision](vision.md)).
- **Streaming:** assets load by region, ahead of the player.
- **What lasts.** Characters, their inventories and levels, guild holdings and the world's state last through checkpoints; scripts never write to a database. A character's login is a carry load; admin commands, purchases and calendar events are recorded inputs. Content upgrades apply at restart, and migrations convert saved script state. An item leaves for another world by burn and attest ([Item export](../05-protocol-spec.md#item-export)), and a world keeps the ids it redeemed as state.

| In the sim (game state) | Outside the sim (server services) |
| --- | --- |
| Trade, parties, guild ownership, quests, faction relations | Chat, mail, friend lists |

## State and derived

- **State:** everything of the world, in its checkpoints; which regions sleep, and since when.
- **Derived:** which regions touch, for the parallel stages.

## Cost

A sleeping region costs nothing a tick; a waking one its `on_wake` call. A checkpoint copies only what changed since the last.

## Genres

WoW and Lineage; Skyrim and every open-world RPG; any game whose world is larger than its players' reach.
