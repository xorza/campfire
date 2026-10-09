# Games

A game is a package: the capabilities it declares, its data and its mode scripts. Nothing ties a capability to a genre. The engine grows for the games it imports, each from the player's own copy, through a module of the `import` app and a rules package ([Zero Hour](../12-zero-hour.md)).

## Imported games

In the order they come:

| Game | Capabilities | Rules package | Tick rate |
| --- | --- | --- | --- |
| Command & Conquer: Generals – Zero Hour | `combat`, `stats`, `abilities`, `projectiles`, `areas`, `orders` (groups), `navigation` (ground and air, Zero Hour's pathfinder and locomotors as its pathfinding and movement backends), `vision` (grid fog), `production`, `interaction` (garrisons, transports), `progression` (veterancy) | General's powers and sciences, money over time, superweapons, the skirmish AI | 30 Hz |
| Worms 4: Mayhem | `combat`, `stats`, `projectiles` (gravity, wind), `areas`, `character`, `physics`, destructible terrain, the `spatial` metric | Turns, the weapons, the missions | Not yet known |

Match phases, win conditions, economy and content are mode scripts in every game.

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

## Main risks

- **Unit counts:** a large Zero Hour battle, eight armies on one map, at 30 Hz must fit the tick budget and the bandwidth; the kernel benches measure 1,000 units ([Benches](../02-engine-core.md#benches)). The server sends only what each group sees, so relevance and delta updates carry the load.
- **Rhai's speed:** the modules only Zero Hour has run as Rhai; their cost at a battle's scale is measured before the rules package grows.
- **Parity:** a difference a player notices, in the pathfinder, a locomotor or a formula, shows only where the oracle's replays reach it.
- **No copy in CI:** an imported package exists only on a machine that holds the game.

## Mixed

A first-person commander, for example, drives a `character` with `hitboxes` weapons, orders squads through `orders`, and fights units that walk `navigation` paths: one package, one tick rate, the union of what its parts need.
