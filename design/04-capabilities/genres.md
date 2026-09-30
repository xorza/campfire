# Genres

A genre is a package: the capabilities it declares, its data and its mode scripts. These are the ones the design has in view; a mode may combine them in any other way.

| Genre | Capabilities | Tick rate |
| --- | --- | --- |
| MOBA | `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders`, `navigation` (grid, paths), `vision` (grid fog, stealth) | 30 Hz |
| RTS | `combat`, `stats`, `orders`, `projectiles`, `navigation` (grid), `vision` (grid fog) | 10–30 Hz |
| FPS | `combat`, `character`, `hitscan`, `vision` (3D occlusion), `navigation` (navmesh, for bots) | 64–128 Hz |
| MMO | `persistence`, `combat`, `stats`, `abilities`, `character` or `orders`, `navigation`, `vision` (relevance) | 10–20 Hz |
| Battle royale | The FPS set, `projectiles` (falling bullets), `physics`, `vision` (relevance) | 30–60 Hz |

Match phases, win conditions, economy and content are mode scripts in every genre.

## MOBA

The reference game: [Reference MOBA](../07-reference-moba.md). Mode scripts run hero pick, creep waves on each lane, gold and experience, and respawns; the creep, tower and camp AI are unit scripts.

## RTS

One player orders many units; selection is client-side, and an order names the units it goes to. Economy, production queues and tech are mode scripts with timers and per-player resources. Building placement needs grid occupancy from `navigation`, not designed yet.

## FPS

Rounds, buy time, the bomb, economy and team swaps are mode scripts, built on core primitives (timers, freeze, respawn, team changes).

## MMO

A persistent world: low tick rate, strict relevance, dormant regions. Payments are time-based and per event; no wagers.

## Battle royale

Around 100 players per match. The plane drop, the shrinking zone, squads and the last team standing are mode scripts; loot spawns from the seeded RNG, stays dormant until someone is near, and is sent only to nearby clients. Wagers and entry fees work as in any match.

**Main risk:** performance, 100 players at 30–60 Hz with vehicles, bullets and loot in one deterministic process. It needs deterministic multithreading and early `det-ci` benchmarks.

## Mixed

Nothing ties a capability to a genre. A first-person commander, for example, drives a `character` with `hitscan` weapons, orders squads through `orders`, and fights creeps that walk `navigation` paths: one package, one tick rate, the union of what its parts need.
