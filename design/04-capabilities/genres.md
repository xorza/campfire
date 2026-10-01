# Genres

A genre is a package: the capabilities it declares, its data and its mode scripts. Nothing ties a capability to a genre. The engine is complete when community packages can rebuild each target game below.

## Target games

| Game | Capabilities | Mode scripts | Tick rate |
| --- | --- | --- | --- |
| League of Legends | `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders`, `navigation` (grid, paths), `vision` (grid fog), `items`, `progression` | Pick, creep waves, gold, respawns, objectives | 30 Hz |
| StarCraft | `combat`, `stats`, `abilities`, `projectiles`, `orders` (groups, queues), `navigation` (grid, air and ground, flow fields), `vision` (grid fog), `production` | Races, supply, win by destruction | 16–24 Hz |
| C&C Generals | The StarCraft set, `interaction` (garrisons, transports), `progression` (veterancy) | Generals' powers, money over time, superweapons | 15–30 Hz |
| Counter-Strike | `combat`, `stats`, `character`, `hitscan`, `projectiles` (grenades), `areas`, `vision` (3D occlusion, smoke), `items` (weapons, buying), `interaction` (bomb, doors), level geometry | Rounds, buy time, economy, bomb, team swaps | 64–128 Hz |
| PUBG | The Counter-Strike set, `physics` (vehicles), `vision` (relevance), `items` (loot) | Plane drop, shrinking zone, squads, last team standing | 30–60 Hz |
| WoW, Lineage | `persistence`, `combat`, `stats`, `abilities`, `character` or `orders`, `navigation` (navmesh), `vision` (relevance), `items`, `progression`, `interaction` (NPCs, quests) | Zones, dungeons, quests, loot, factions, guild wars, sieges | 10–20 Hz |

Match phases, win conditions, economy and content are mode scripts in every genre.

## The genres in the model

Each genre says its rules in the same terms ([The model](00-overview.md#the-model)):

| | MOBA | FPS | RTS | MMO | Battle royale |
| --- | --- | --- | --- | --- | --- |
| Control | Orders | Input frames | Orders, groups | Input frames or orders | Input frames |
| Actions | Attacks, abilities, item actives | Guns (rays), grenades, the bomb | Weapons, abilities, train, build, gather | Spells, auto attack, use | Guns, consumables |
| Pools | Health, mana or energy | Health, armor; ammunition on guns | Health, energy, shields | Health, mana, rage, combo points | Health, armor; ammunition |
| Tags | Crowd control, stealth and true sight | Flashed, smoke | Cloak and detection, air and ground | Crowd control, immunities, stealth | Downed |
| Relations | Two teams; camps on a hostile team | Two teams | Two to eight teams | Factions; neutral monsters | Up to 100 squads |
| Space | Planar, one layer | Spatial | Planar, ground and air | Planar or spatial | Spatial |
| Vision | Grid fog | 3D occlusion | Grid fog | Relevance | Relevance |
| Markers | Spawns, camps | Bomb sites, buy zones | Start locations, resource fields | Zones, quest givers | Loot spots, zone centers |
| Choices | Hero, spells | Side, loadout | Faction | Character | Loadout |

A battle royale's shrinking zone is an area unit that damages, through a modifier, the units outside it; its loot is item units spawned at markers. An MMO's quest giver is a unit whose `use` action opens a dialogue.

## Genre proofs

Before the MOBA is complete, each target game gets a tiny test mode, not a game, that `det-ci` runs. Each uses only the capabilities of its row, so a MOBA-only choice fails a test early. With the reference MOBA, they cover most pairs of capabilities; small mixed modes cover the rest ([Testing combinations](00-overview.md#testing-combinations)).

| Proof | Shows |
| --- | --- |
| One CS round | A driven character, hitscan with lag compensation, a bought weapon, planting the bomb |
| An RTS skirmish | Harvesting, a build queue, placing a building, a group order, an air unit over a ground unit |
| A BR zone | Loot on the ground, a shrinking zone, 100 one-player teams, relevance over a large map |
| An MMO zone | A saved character, a level-up, an NPC quest, a neutral monster, a dormant region |

## Main risks

- **Unit counts:** an RTS battle with 1,000+ units must fit the tick budget and the bandwidth. The server sends only what each group sees, so relevance and delta updates carry the load.
- **Player counts:** 100 players in battle royale, and 1,000+ in one MMO session, need deterministic multithreading and early `det-ci` benchmarks.
- **Shooter feel:** 64–128 Hz, and lag compensation that the verifier reproduces.

## Mixed

A first-person commander, for example, drives a `character` with `hitscan` weapons, orders squads through `orders`, and fights units that walk `navigation` paths: one package, one tick rate, the union of what its parts need.
