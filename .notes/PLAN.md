# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](../AGENTS.md#workflow).

## Shroud and minimap

The slice of [Shroud and minimap](../docs/design/shroud.md): a player of a Zero Hour skirmish sees the cells their team never saw black and those it saw before dark, the buildings it saw there as ghosts, no enemy their team does not see, and, once they hold a radar, a minimap they move the camera and order from. Its facts are [Zero Hour's shroud and radar](../docs/research/zero-hour-shroud.md); its checks are that document's.

### SM1. Zero Hour sees

Touches the Zero Hour import and its check; it stands first, as each later batch shows the fog of a Zero Hour match.

- [ ] The imported mode sees everything (shroud.md, Zero Hour sees by the vision capability): the rules package's mode holds no `vision` (`rules_package/mod.rs:142`), a map writes no `[grid]` (`map_import/mod.rs:350`), a type no `[vision]` (`imported_type/mod.rs:485`), and the object INI passes over `ShroudClearingRange` (`object_ini/mod.rs:1439`). The mode gains `vision`, each map a grid of 40 m cells over its bounds, and each type its `sight_range` of `ShroudClearingRange`, none for a range of 0 or less. Checks: the import's tests give a type its sight, a map its grid and the mode its vision; the Zero Hour check pins the types of each sight, its golden moves for the plays a team's sight changes, each move named, and its bots' match still ends.

### SM2. A team's cells reach its player

Touches vision and the visibility kinds; it stands after SM1, so the LAN check and the Zero Hour check measure a match of fog.

- [ ] A team keeps no cell it saw before, and its client holds none (shroud.md, A team keeps the cells it explored; shroud.md, The server sends a team's cells to its player): `Fog` counts the cells each group sees now (`fog/mod.rs:5`), `Seeing::see` writes each unit's `SeenBy` alone (`seeing.rs:43`), and a predicting client's `ByHolding::sees_point` answers true (`team_sight.rs:68`). A map's grid gains `shroud`; each player's entity gains the cells its team sees and, of a shrouded map, those it explored, of a new kind `sight` sent to its owner alone (`data_kind.rs:57`), written only when a cell changes; a refill takes the explored cells from them; and `ByHolding::sees_point` reads them. Checks: the vision tests count a group's explored cells, keep them over a save and a restore, and write a player's cells only on a change; the leak test holds `sight` to its owner; the LAN check passes and its log gives the bytes of `sight` sent.

### SM3. The fog drawn

Touches the client files, the Zero Hour import and the client's material; it stands after SM2, whose cells it draws.

- [ ] The client draws every cell as seen (shroud.md, The fog is the one material's): `FileBlend` holds no fog (`file_material/mod.rs:34`), and no client file the levels of one. A file `client/fog.toml` gives the levels of a cell seen, explored and shrouded and the fog's color, the import writing `GameData.ini`'s `ShroudColor`, `ClearAlpha`, `FogAlpha` and `ShroudAlpha` (`game_data_ini/mod.rs`); the client keeps one texture of a texel for each grid cell, written in place as its player's cells change, and the file material multiplies its color by its level, shrouded past the grid and clear in a mode of no vision. Checks: the client files' tests read and refuse the file; the client's tests give each texel's level of a player's cells and a point's place on the texture.

### SM4. Ghosts

Touches the client; it stands after SM3, as a ghost stands in the fog it draws.

- [ ] A building that leaves the client goes from the view (shroud.md, An immobile unit leaves a ghost): a unit that leaves takes its drawing with it, and only a death's fall tears it off (`drawing.rs:36`). A unit of a type of no movement that leaves, not dying, leaves its drawing as a ghost, as it stood, which goes when its player's cells see its cell, and is not picked, selected or barred. Checks: the client's tests spawn a ghost of an immobile unit that leaves and none of one that dies or moves, and drop it when its cell is seen.

### SM5. The minimap

Touches the HUD file, the Zero Hour import and the client; it stands last, as it draws the fog, the ghosts' cells and the units the batches before give.

- [ ] Nothing gives a player a radar (shroud.md, A minimap by a radar tag): the import reads a `RadarUpgrade` as an upgrade of no effect (`ini_upgrade_module.rs:72`), and the HUD file names no minimap (`hud_file/mod.rs:14`). A `RadarUpgrade` gives its object the tag `radar` once its trigger is researched, as a look's tag of an upgrade (`upgrade_import/mod.rs:289`); the HUD file gains `minimap`: its radar tag, its cells, its bright and dark shares and its dots' size, the import writing Zero Hour's. Checks: the import's tests give a `RadarUpgrade` its tag; the client files' tests read and refuse the minimap.
- [ ] The client shows no map (shroud.md, The minimap is drawn from the terrain at load; shroud.md, A minimap click moves the camera, a right click orders a move): the HUD holds bars, the hit mark and the panel. The client derives the minimap's terrain from the composed terrain texture and the heights as it loads (`terrain_image.rs:39`), darkens it by its player's cells, draws each unit it holds and sees as a square of its team's color and the camera's view as its trapezoid, shows it while its player holds a unit of the radar tag, moves the camera to a clicked point (`camera_rig/mod.rs:190`) and orders the selection's move at a right-clicked one. Checks: the client's tests shade a cell's terrain, place a unit and the view's corners on the minimap, take a click to its point on the ground, and show the minimap by the radar tag.

### End of the slice

It stands last, as the player's check is of the whole slice.

- [ ] A player of Tournament Desert scouts the enemy's base under the shroud, sees its buildings stay as ghosts when the scout leaves, researches the radar and moves the camera and orders a move from the minimap, in the client; then the slice's decisions fold into `04-capabilities/vision.md`, `10-replication-audiences.md`, `02-engine-core.md` and `09-zero-hour.md`, and `shroud.md` goes.

## Foundations
