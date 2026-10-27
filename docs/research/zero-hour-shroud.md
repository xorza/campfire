# Zero Hour's shroud and radar

What the released source (GeneralsX, read as a specification) and the game's INI files say of the shroud, the fog, the ghosts of objects and the radar, and what other engines do. Read 27 October 2026.

## The shroud

- **Three states for each cell and player.** `PartitionManager` keeps, for each partition cell and player, `CELLSHROUD_CLEAR` (a looker sees it now), `CELLSHROUD_FOGGED` (seen before) and `CELLSHROUD_SHROUDED` (never seen).
- **The cell is the partition's.** `GameData.ini` gives `PartitionCellSize = 40.0`; `W3DShroud::init` takes the partition's cell size, so the drawn shroud has one texel for each 40-unit cell.
- **The levels.** `GameData.ini`: `ShroudColor = R:255 G:255 B:255`, `ClearAlpha = 255`, `FogAlpha = 127`, `ShroudAlpha = 0` ("0 is opaque, 255 is clear"). `W3DShroud::setShroudLevel` writes each texel as the shroud color times its level, and the terrain is multiplied by the texture, filtered between texels.
- **No fade.** `W3DShroud::interpolateFogLevels` moves each texel toward its level at `255/1000` a millisecond, but only under `DO_FOG_INTERPOLATION`, which no file defines: a cell's level changes at once.
- **The border.** `m_boderShroudLevel` starts at `m_shroudAlpha`: the map past its playable area is drawn shrouded; a script may set it to `m_clearAlpha` (`ScriptActions.cpp`).
- **Objects in the fog.** `RTS3DScene::updateFixedLightEnvironments` lights a fogged object by the ambient light times `FogAlpha / ClearAlpha`; `W3DShroudMaterialPassClass` applies the shroud texture to objects as to the terrain.
- **Sight.** An object reveals by its `ShroudClearingRange` (`Object::getShroudClearingRange`); a structure under construction reveals only its bounding circle's radius. `VisionRange` is a separate field, the range of the AI's targeting. A GLA vehicle of `GLAVehicle.ini` gives `VisionRange = 125`, `ShroudClearingRange = 300`.
- **A build needs a clear cell.** `BuildAssistant::isLocationLegalToBuild`, with `SHROUD_REVEALED`, refuses a point whose cell is not `CELLSHROUD_CLEAR`, as its first test.

## Ghosts

- **Immobile objects leave a ghost.** `PartitionData::attachToObject` makes a `GhostObject` for an object of `KINDOF_IMMOBILE` whose draw modules are not `W3DDefaultDraw`. A ghost draws the object as the player last saw it while its cells are fogged for that player, and goes when the player sees them again.

## The radar

- **Size.** `Radar.h`: `RADAR_CELL_WIDTH = 128` and `RADAR_CELL_HEIGHT = 128`, over the map's extent.
- **The terrain.** `W3DRadar::buildTerrainTexture` gives each radar cell the average of the terrain's color at 3 × 3 sample points, and `interpolateColorForHeight` brightens it toward 0.95 above the map's middle height and darkens it toward 0.60 below; a cell under water takes the water's radar color, shaded by its depth.
- **Objects.** `W3DRadar::renderObjectList` draws each object as 2 × 2 texels of its color. `W3DRadar::canRenderObject` draws no object fogged or shrouded for the local player, an object of `RadarPriority = LOCAL_UNIT_ONLY` only for its controller, and no stealthed enemy that is not detected. An object of `RadarPriority = NOT_ON_RADAR` is never drawn.
- **The view box.** `W3DRadar::reconstructViewBox` projects the 4 corners of the view onto the map at its average height and draws their trapezoid.
- **Shroud on the radar.** `W3DRadar::setShroudLevel` darkens each radar cell by its player's shroud state.
- **A radar is an upgrade.** `FactionBuilding.ini`: `AmericaCommandCenter` and `ChinaCommandCenter` each hold `RadarUpgrade` triggered by `Upgrade_AmericaRadar` and `Upgrade_ChinaRadar`, and `RadarUpdate` with `RadarExtendTime = 4000`, the dish's animation; `GLAVehicle.ini`'s radar van holds `RadarUpgrade` triggered by `Upgrade_GLARadar` with `DisableProof = Yes`. 14 object files hold a `RadarUpgrade` or a `RadarUpdate`.

## Other engines

- **League of Legends.** Its server withholds an enemy's place until a client must show it ([Riot, VALORANT's fog of war](https://technology.riotgames.com/node/108)). Its fog was a 128 × 128 grid over the map, blurred by a Gaussian and upscaled to 512 × 512 to draw, with one visibility flag for each cell in place of a gradient ([Riot, A story of fog and war](https://www.riotgames.com/en/news/story-fog-and-war)).
- **OpenRA.** A third state of explored cells comes with its shroud, and a lobby option starts the map explored ([OpenRA forum](https://forum.openra.net/viewtopic.php?p=310577)).
- **Dota 2.** No source of Valve's says which side computes the fog it draws.
