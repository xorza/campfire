# Genres

A genre is a package: the capabilities it declares, its data and its mode scripts. Nothing ties a capability to a genre. The engine is complete when community packages can rebuild each target game below.

## Target games

| Game | Capabilities | Mode scripts | Tick rate |
| --- | --- | --- | --- |
| League of Legends | `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders`, `navigation` (grid, paths), `vision` (grid fog, stealth), `items`, `progression` | Pick, creep waves, gold, respawns, objectives | 30 Hz |
| StarCraft | `combat`, `stats`, `abilities`, `projectiles`, `orders` (groups, queues), `navigation` (grid, flow fields), `vision` (grid fog), `production` | Races, supply, win by destruction | 16–24 Hz |
| C&C Generals | The StarCraft set, `interaction` (garrisons, transports), `progression` (veterancy) | Generals' powers, money over time, superweapons | 15–30 Hz |
| Counter-Strike | `combat`, `character`, `hitscan`, `projectiles` (grenades), `areas`, `vision` (3D occlusion, smoke), `items` (weapons, buying), `interaction` (bomb, doors), level geometry | Rounds, buy time, economy, bomb, team swaps | 64–128 Hz |
| PUBG | The Counter-Strike set, `physics` (vehicles), `vision` (relevance), `items` (loot) | Plane drop, shrinking zone, squads, last team standing | 30–60 Hz |
| WoW, Lineage | `persistence`, `combat`, `stats`, `abilities`, `character` or `orders`, `navigation` (navmesh), `vision` (relevance), `items`, `progression`, `interaction` (NPCs, quests) | Zones, dungeons, quests, loot, guild wars, sieges | 10–20 Hz |

Match phases, win conditions, economy and content are mode scripts in every genre.

## Genre proofs

Before the MOBA is complete, each target game gets a tiny test mode, not a game, that `det-ci` runs. Each uses only the capabilities of its row, so a MOBA-only choice fails a test early.

| Proof | Shows |
| --- | --- |
| One CS round | A driven character, hitscan with lag compensation, a bought weapon, planting the bomb |
| An RTS skirmish | Harvesting, a build queue, placing a building, a group order |
| A BR zone | Loot on the ground, a shrinking zone, relevance over a large map |
| An MMO zone | A saved character, a level-up, an NPC quest, a dormant region |

## Main risks

- **Unit counts:** an RTS battle with 1,000+ units must fit the tick budget and the bandwidth. The server sends only what each team sees, so relevance and delta updates carry the load.
- **Player counts:** 100 players in battle royale, and 1,000+ in one MMO session, need deterministic multithreading and early `det-ci` benchmarks.
- **Shooter feel:** 64–128 Hz, and lag compensation that the verifier reproduces.

## Mixed

A first-person commander, for example, drives a `character` with `hitscan` weapons, orders squads through `orders`, and fights units that walk `navigation` paths: one package, one tick rate, the union of what its parts need.
