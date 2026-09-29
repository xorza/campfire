# Campfire — Game Kits

A kit is a native Rust crate that adds one genre to the engine core: components, systems that run in the tick, backends, and the script API for that genre. A game package declares the kits it uses. Kits follow the core's determinism rules and ship in engine releases; packages cannot add native code.

| Kit | Adds | Status |
| --- | --- | --- |
| [MOBA](moba.md) | Orders, abilities, modifiers, lanes, grid pathfinding, grid fog of war | First |
| [FPS](fps.md) | Input frames, character controller, hit tests, lag compensation, hitboxes, 3D occlusion, navmesh | Later |
| [MMO](mmo.md) | Persistence helpers, dormancy, region streaming, inventory, quests | Later |
| [Battle royale](battle-royale.md) | Builds on FPS: vehicles, ballistics, terrain, long-range relevance, loot | Later |

Match phases, win conditions, economy and content are mode scripts in every kit.
