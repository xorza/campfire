# Zero Hour — research

What an import of Command & Conquer: Generals – Zero Hour reads, measured on a Steam install and read from the released source, for [Zero Hour](../design/12-zero-hour.md). Facts only: what the design takes from them is its own. Measured on 10 October 2026; the source is [TheSuperHackers/GeneralsGameCode](https://github.com/TheSuperHackers/GeneralsGameCode) at `ab568af4` (9 October 2026), which every path below names.

## The install

The Steam install keeps Zero Hour in its root and base Generals in `ZH_Generals/`. Zero Hour reads both: its own archives first, then base Generals' (`StdBIGFileSystem::init`).

- **Which archive wins.** The game loads every `.big` under its directory, subdirectories included, in case-insensitive order of path, and the first archive that holds a path keeps it: `loadBigFilesFromDirectory` passes `overwrite = FALSE` (`Core/GameEngineDevice/Source/StdDevice/Common/StdBIGFileSystem.cpp`, `Core/GameEngine/Include/Common/FileSystem.h`). `ZH_Generals/` sorts last, so base Generals fills only what Zero Hour lacks; so does a patch archive that sorts after the file it patches, as `PatchINI.big`'s `WindowTransitions.ini` loses to `INIZH.big`'s.
- **A duplicate to skip.** English, Chinese and Korean versions shipped a second `INIZH.big` in `Data/INI/`, which the retail patch deleted at start. It differs from the root's: 99 files against 135, and some of the same paths at other sizes. The source skips it by name.
- **Loose files.** `Data/` holds cursors, movies, water textures and `Data/Scripts/` (`SkirmishScripts.scb`, `MultiplayerScripts.scb`, `Scripts.ini`), outside every archive.

35 archives, 25,294 files once resolved, 970 of them shadowed:

| Kind | Files | Size | Note |
| --- | --- | --- | --- |
| `.w3d` models | 8,901 | 353 MiB | 6,679 named by the INI |
| `.wav` sounds | 8,475 | 806 MiB | |
| `.dds` textures | 6,602 | 317 MiB | |
| `.tga` textures | 740 | 219 MiB | |
| `.ini` | 222 | 18.5 MiB | 135 Zero Hour files, and the base maps' `map.ini` |
| `.map` | 150 | 46 MiB | packed |
| `.wnd` window layouts | 80 | 8.1 MiB | |
| `.mp3` music | 56 | 183 MiB | |

## Archives

`BIGF`, a `u32` little-endian archive size, a `u32` big-endian file count and a `u32` big-endian header size; then one entry per file, a `u32` big-endian offset and size and a NUL-terminated path. Paths use `\` and mixed case, so a lookup folds case.

## INI

- **Size.** Zero Hour's 135 files: 17.9 MiB, 12.9 MiB without comments and blank lines. The largest without comments: `ParticleSystem.ini` 1.4 MiB, `Object/FactionBuilding.ini` 1.1 MiB, each general's object file about 0.5 MiB.
- **Objects.** 1,795 `Object` blocks under `Data/INI/Object/`, built of 201 module types: an object is a list of modules, each `Behavior =`, `Body =`, `Draw =`, `ClientUpdate =` with its fields.

| Module | Objects | Module | Objects |
| --- | --- | --- | --- |
| `Draw: W3DModelDraw` | 1,411 | `Behavior: GarrisonContain` | 200 |
| `Body: ActiveBody` | 956 | `Behavior: ProductionUpdate` | 174 |
| `Behavior: FXListDie` | 758 | `Behavior: StealthUpdate` | 114 |
| `Behavior: FlammableUpdate` | 612 | `Behavior: OCLSpecialPower` | 59 |
| `Behavior: PhysicsBehavior` | 497 | `Behavior: TransportContain` | 54 |
| `Behavior: AIUpdateInterface` | 353 | `Behavior: MissileAIUpdate` | 40 |
| `Body: StructureBody` | 264 | `Behavior: JetAIUpdate` | 32 |

- **Object files.** `ThingFactory` loads `Data\INI\Default\Object.ini` and its folder, then `Data\INI\Object.ini` and its folder. A comment starts at `;` alone; `//` is a token. `=` is a separator, not a requirement.
- **Default models.** The objects' default states name 1,197 models: 1,181 convert, 2 draw nothing (`NULL` and `CINEExplBox`, a box alone), 13 name no file of the archives (the AI tiles among them), and one, `EXHydrant`, is a particle emitter. `NULL` names the render object `WW3DAssetManager::Find_Prototype` always has, which draws nothing.
- **Times and speeds** (`Core/GameEngine/Source/Common/INI/INI.cpp`). The logic runs 30 frames a second (`WWSyncPerSecond`, `Core/Libraries/Source/WWVegas/WWLib/WWCommon.h`). A duration in milliseconds becomes whole frames as `ceilf(ms × LOGICFRAMES_PER_MSEC_REAL)`, in 32-bit floats, `parseDurationUnsignedInt`, or stays a fractional count of frames, `parseDurationReal`. Velocities, accelerations and angular velocities become per-frame floats. So the original's values are not the decimals the INI writes, but what those float steps make of them.

## Sizes and ranges

In the original's world units; the map's cells are 10 of them ([Maps](#maps)).

A body's size is a box's half-diagonal, or a cylinder's or a sphere's radius.

| What | Smallest | Median | Largest |
| --- | --- | --- | --- |
| Infantry | 1 | 10 | 10 |
| Vehicles | 0 | 18 | 119, a rocket train |
| Aircraft | 0 | 20 | 61, a B-52 |
| Structures | 0 | 49 | 1,117, a bridge; 460, the large pyramid |
| Weapon `AttackRange` | 0 | 150 | 10⁶, a marker for "anywhere" |
| Locomotor `Speed`, per second | 0 | 60 | 1,200 |
| `VisionRange` | 0 | 180 | 99,999 |

Geometry is `BOX` on 710 objects, `CYLINDER` on 394, `SPHERE` on 73, and none on 616.

## Maps

- **Which maps.** The game lists each `.map` under `Maps\` in a folder of its own name, `Maps\<name>\<name>.map` (`MapCache::loadMapsFromDisk`, `Core/GameEngine/Source/GameClient/MapUtil.cpp`); all 150 are.
- **Packing** (`Core/Libraries/Source/Compression/CompressionManager.cpp`). A four-byte tag, the `u32` little-endian unpacked size, then the stream: `EAR\0` EA's RefPack (`Core/Libraries/Source/Compression/EAC/refdecode.cpp`) on 145 maps, `ZL5\0` a zlib stream on 1, and no tag on 4, which are `CkMp` as they are. The game also reads `ZL1` to `ZL9`, `NOX`, `EAB` and `EAH`, which no shipped map uses.
- **Versions.** `HeightMapData` 4 and `ObjectsList` 3, of `Object` chunks 3, on every map; `BlendTileData` 6 on 3 maps, 7 on 31 and 8 on 116. Version 6 stores no cliff bits, which the game makes from the heights: a cell is a cliff when its corners span more than 9.8 units. Version 7 stored rows of `(width + 1) / 8` bytes of them, short by a byte for most widths.
- **Chunks** (`GeneralsMD/Code/GameEngine/Source/Common/System/DataChunk.cpp`). `CkMp`, a table of chunk names, each a length byte, the name and a `u32` id; then chunks, each a `u32` id, a `u16` version and an `i32` size, little-endian. A dictionary is a `u16` count of entries, each a key id and type in one `i32`, then a bool, `i32`, `f32`, string or wide string.
- **Top chunks.** `HeightMapData`, `BlendTileData`, `WorldInfo`, `SidesList`, `ObjectsList`, `PolygonTriggers`, `GlobalLighting`, `WaypointsList`.
- **Heightmap** (`WorldHeightMap::ParseHeightMapData`). Width, height, border, the playable boundaries, then one byte of height per cell. A cell is `MAP_XY_FACTOR` = 10 units wide, and a height step `MAP_HEIGHT_SCALE` = 0.625 units (`Core/GameEngine/Include/Common/MapObject.h`); both are exact in binary.
- **Objects** (`WorldHeightMap::ParseObjectData`). Each a position of three `f32`, an angle `f32`, flags `i32`, a template name and a dictionary of properties: owner, initial health, `uniqueID`, waypoint ids among them.
- **Two maps measured:**

| Map | Cells | Border | Objects, kinds | Unpacked |
| --- | --- | --- | --- | --- |
| Tournament Desert | 270 × 340 | 35 | 784, 68; 28 waypoints | 1.0 MiB |
| China, final mission | 370 × 570 | 70 | 2,683, 180 | 3.6 MiB packed |

- **All maps.** Unpacked, 2.2 MiB at the median, 6.4 MiB at most (`md_gla05`), 342 MiB for all 150. `BlendTileData` is most of each.
- **Objects only the client shows** (`GameLogic::startNewGame`). A road's or a bridge's end and an object whose template no INI object names spawn nothing. An optimized tree (`KINDOF_OPTIMIZED_TREE`) gets a drawable that registers it in the client's tree buffer, then goes; a prop (`KINDOF_PROP`), and fluff, an object with `KINDOF_CLEARED_BY_BUILD` and no fence width, go to the client's prop buffer when `forceFluffToProp` holds; neither has a logic object. A shrub (`KINDOF_SHRUBBERY`) spawns only with trees on. In a multiplayer game, `TheRecorder->isMultiplayer()`, trees are on and fluff goes to the client, whatever the options; in any other, `m_useTrees` and the static detail level decide, fluff a logic object at the high level or the custom one with shadow volumes. The client removes props and trees under a new structure (`removePropsForConstruction`, `removeTreesForConstruction`), and its tree buffer pushes trees aside and topples them as units move (`W3DTreeBuffer::unitMoved`).
- **Their objects.** 281,480: 82,397 ends of roads and bridges, 25,463 waypoints, and 173,620 others.
  - **Waypoints.** Ids 1 to 909, none twice on a map; every name ASCII, none a number; one name on several waypoints of a map on 66 maps.
  - **Heights.** None past the range the game drops at −1,000 and 1,593.75. 6,261 of the others stand off the ground, from −89 to 1,350, −4 at the median, most of them props sunk into it.
  - **Angles.** 562 lie past (−π, π], up to 4.73, which the game's `normalizeAngle` brings back by a turn.
  - **Places.** 232, all on campaign maps, lie past their heightmap's extent.
  - **Names.** 1,439 templates, of which none differ only in case; one non-ASCII name, on a road's end.
- **Their terrain.** Each blended tile sets exactly one of its four directions; its inverted byte holds 0 to 3, the inverted and the flipped bit. No map has an edge class. 13 maps hold no cliff UVs, and 0, which names none, in every cell.

## Terrain

`BlendTileData` (`WorldHeightMap::ParseBlendTileData`) holds, for every cell, a tile, a blend tile, a third blend tile and a cliff UV index, each an `i16`, and a bit for each cell's cliff state. Then the texture classes, each its first tile, its count, its width in tiles and its name, which `Terrain.ini` maps to a TGA of `Terrain.big`, cut into tiles; the edge classes; and the blended tiles, each the tile it blends and its direction: horizontal, vertical, either diagonal, inverted, long diagonal, and a custom edge class. A blended tile of a custom edge class draws nothing: `WorldHeightMap::getAlphaUVData` sets its four corners' alpha to 0, and the edge texture the game builds from the edge classes (`AlphaEdgeTextureClass::update`) no renderer binds, as nothing calls `getEdgeTerrainTexture`.

## Models

W3D is chunks (`Generals/Code/Libraries/Source/WWVegas/WW3D2/w3d_file.h`): a `u32` type and a `u32` size, whose top bit marks a chunk of chunks.

| File | Count |
| --- | --- |
| Model of meshes on a hierarchy (`HLOD`) | 6,504 |
| Meshes alone | 1,172 |
| Animations | 1,148, 21 of them compressed |
| Hierarchies alone | 73 |
| Particle emitters | 3 |

- **Meshes.** 70,571, 18 vertices at the median and 1,645 at most. Each has vertex materials, shaders and texture stages in material passes; 423 have vertex colors, 171 are skinned, and 1,435 carry an axis-aligned box tree for picking.
- **Textures they name.** 5,639, of which 55 are in no archive.
- **Names.** A mesh's prototype is `Container.Mesh`, or `Mesh` alone when its container's name is empty (`MeshModelClass::read_chunks`); 338 of the models the objects name are such a mesh alone.
- **HLOD sub-objects.** 70,124: 69,399 meshes and 680 collision boxes of their own file, and 45 names of no render object, which `HLodClass` skips. No HLOD has more than one level of detail, and none has an aggregate. No code of the game sets `BoxRenderObjClass::Set_Box_Display_Mask`, so boxes are never drawn.
- **Lists of a pass.** A one-id list of vertex materials, shaders or textures is one for all (`read_vertex_material_ids`, `read_shader_ids`, `read_texture_ids`). A triangle's texture id `0xFFFFFFFF` leaves it untextured. The stage then holds no texture (`TextureBaseClass::Apply_Null`), and native Direct3D's fixed function samples a stage of no texture as the null vector, `(0, 0, 0, 0)`, which DXVK, the layer GeneralsX draws through, follows ([DXVK commit 0b49a39](https://github.com/doitsujin/dxvk/commit/0b49a39), "Return null vector when sampling null texture in fixed-function"). A second chunk of a list in one pass or stage goes to the alternate material description, which a draw's default state does not show. Some texture stages hold texture ids and no coordinates.
- **Vertex colors.** `MeshMatDescClass::Post_Load_Process` moves a pass's vertex colors to the light its vertex materials use: with diffuse light, and ambient or not, they are multiplied by the diffuse color and the opacity and become the diffuse and ambient color; with ambient alone, the ambient; with emissive alone, the diffuse, with lighting off. All 69 meshes of converted models with vertex colors use diffuse and ambient light.
- **Materials.** Of the vertex materials of converted models, 336 have an ambient color other than their diffuse one. 275 meshes of converted models have a second material pass; none has no pass.
- **Lighting, all models.** Of the install's shaders, 3,247 turn the primary gradient off, which draws the texture with no lighting, and 74,409 modulate it; none turns the secondary gradient, the specular, on, though 25,351 vertex materials hold a specular color. 317 first passes hold a second texture stage, a detail stage, which 315 shaders mix by their detail functions: 105 scale the color, 66 add it, and 144 take the alpha of the detail; 2,155 first passes hold no stage.
- **Scale and state.** Every drawable takes its template's `Scale`, its asset scale, 1 by default (`Drawable`'s `m_instanceScale`, `W3DModelDraw`'s render objects); a prop of the client takes it too, and the model its first draw module gives for the map's state, `NIGHT` on a night map and `SNOW` on a snowy one (`W3DTerrainVisual::addProp`, `getBestModelNameForWB`); a unit's drawable takes `NIGHT` by its time of day (`Drawable::setTimeOfDay`). Of the 150 shipped maps, 47 are of the morning, 85 of the afternoon, 3 of the evening and 15 of the night, and 17 snowy; Tournament Desert is of the morning, with no snow.
- **Second passes, all models.** Of the install's 70,571 meshes, 5,625 have two material passes and none more. Each second pass is one texture stage, of its own coordinates but in 2, mapped by UV, with no environment mapping. Its shaders: 3,211 additive (source `ONE`, destination `ONE`), 2,395 by alpha (`SRC_ALPHA`, `ONE_MINUS_SRC_ALPHA`), 9 opaque, 7 `ONE` with `ONE_MINUS_SRC_COLOR`, 1 multiplying (`ZERO`, `SRC_COLOR`); all but the 9 opaque write no depth. Of the install's 80,025 vertex materials, 3,094 have an ambient color other than their diffuse one, 134 of them black.
- **Texture coordinates.** 35 meshes of converted models hold `NaN` coordinates at 83 vertices, all on textured triangles; the GPU interpolates `NaN` over each triangle that touches one, so what the game shows depends on the hardware.
- **Missing textures.** A texture no archive holds draws as `MissingTexture`, a solid color of `0x7FFF00FF`. Converted models name one, `dirtground1.bmp`, in 22 meshes of one file.
- **Normals.** 619 vertices in 74 meshes of converted models have a normal of zero; 291 of them lie on a triangle of area. The game lights a vertex per vertex, so such a vertex takes ambient and emissive light alone.
- **Hide and show.** `doHideShowBoneSubObjs` walks each sub-object's bone up to the root: a change to a sub-object on the root spreads to every sub-object off the root.

## Textures

- **DDS.** 6,602: DXT1 3,723, DXT5 2,870, DXT3 9; each with its full chain of levels, its sides multiples of 4, from 4 to 1,024, 256 the commonest, 523 not square; no cube map or volume; each file's bytes exactly its header and its levels.
- **TGA.** 740, all uncompressed true color (type 2) with no color map: 472 of 32 bits and 268 of 24; every one stored bottom row first, and ending with a TGA 2.0 footer that names no extension area. 201 of those of 32 bits say 0 alpha bits in their header; Zero Hour's loader reads every TGA of 32 bits as `A8R8G8B8`, alpha in use (`Get_WW3D_Format`, `Core/Libraries/Source/WWVegas/WW3D2/ww3dformat.cpp`). 36, all terrain textures, have sides of 384 or 640, no power of two. The loader makes a TGA's levels by `BitmapHandlerClass::Combine_A8R8G8B8`, which shifts each of four texels right by 2 before it sums them, so each level darkens by up to 3 of 255.
- **Names.** No `.dds` and `.tga` share a path but for the extension.

## Movement

Read from the released source and the install's `INIZH.big` on 15 October 2026.

- **Locomotor** (`LocomotorTemplate`, `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Locomotor.cpp`). An object names its locomotors by set, `Locomotor = SET_NORMAL <name>`. A locomotor's fields include `Surfaces` (`GROUND`, `WATER`, `CLIFF`, `AIR`, `RUBBLE`), `Speed` and `MinTurnSpeed` in units a second, `TurnRate` in degrees a second, `Acceleration` and `Braking` in units a second squared, `TurnPivotOffset`, `CanMoveBackwards` and `Appearance` (`TWO_LEGS`, `FOUR_WHEELS`, `TREADS`, `HOVER`, `WINGS` and others). The parser turns each per-second value into one per logic frame, of which the game runs 30 a second (`INI::parseVelocityReal`, `Core/GameEngine/Source/Common/INI/INI.cpp`).
- **Body for paths.** A body's bounding circle is `GeometryMajorRadius` for a sphere or a cylinder, and `√(major² + minor²)` for a box (`GeometryInfo::calcBoundingStuff`, `GeneralsMD/Code/GameEngine/Source/Common/System/Geometry.cpp`). The pathfinder sizes a unit by that circle (`Pathfinder::getRadiusAndCenter`, `Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp`): a diameter between one and two cells counts as two, the cells are `floor(diameter / 10 + 0.3)`, at least 1, halved, at most 2.
- **Pathfinder cells** (`Core/GameEngine/Source/GameLogic/AI/AIPathfind.cpp`). A cell is 10 units (`PATHFIND_CELL_SIZE`), one map cell. `classifyMapCell` makes a cell a cliff when the terrain calls its top-left corner a cliff cell (`TerrainLogic::isCliffCell`), and water when any of its corners is under water; objects make obstacle cells later. `classifyMap` then turns each clear cell beside a cliff, of the eight round it, into a cliff, and marks each clear cell beside the grown cliffs as pinched. A locomotor enters a cell only by a surface its type takes (`Pathfinder::validLocomotorSurfacesForCellType`): a clear cell `GROUND` or `AIR`, water `WATER` or `AIR`, a cliff `CLIFF` or `AIR`, rubble `RUBBLE` or `AIR`, an obstacle or an impassable cell `AIR` alone.
- **A skirmish's start** (`placeNetworkBuildingsForPlayer`, `GeneralsMD/Code/GameEngine/Source/GameLogic/System/GameLogic.cpp`). Each player's `PlayerTemplate` gives a `StartingBuilding` and up to ten `StartingUnit` (`PlayerTemplate.cpp`). The building stands at the waypoint `Player_<n>_Start`, `n` the start position counted from 1, turned by its template's `PlacementViewAngle` in degrees. Each starting unit is placed by `PartitionManager::findPositionAround` round the waypoint `Player_<n>_Rally` if the map has one, else round the building's point moved `−y` by half the building's bounding sphere, between 0.7 and 1.3 times that sphere's radius from it.
- **America's start** (`PlayerTemplate.ini`, `Object/FactionBuilding.ini`, `Object/AmericaVehicle.ini`, `Locomotor.ini`). `FactionAmerica` starts with `AmericaCommandCenter` and `AmericaVehicleDozer`. The command center is a box of half extents 60 and 70, 49 high, `PlacementViewAngle = -135`. The dozer is a box of half extents 15 and 10, 15 high, so its bounding circle is 18.03; its locomotor `AmericaVehicleDozerLocomotor` drives on `GROUND` at 30 a second, turns 90° a second, accelerates at 30 and brakes at 50, turns at no less than 25 a second, and can move backwards.
- **Tournament Desert.** Two start waypoints, `Player_1_Start` and `Player_2_Start`; its bounds, as the import writes them, from −350 to 2,340 on x and −3,040 to 350 on z. Its heightmap's playable boundary is 204 × 270 cells within its border of 35. 46,543 of its 91,800 cells are cliffs; grown by one ring, 48,689, which merge row by row into 286 rectangles. 32 of its placed objects are structures that are not small: 4 overlap a grown cliff cell, and 4 pairs overlap each other. Its widest placed walker's bounding circle is 11.7.
- **All 116 maps of `MapsZH.big`.** With cliffs grown by one ring, 2,089 structures that are not small overlap a cliff cell, on 95 maps; 1,474 pairs of them overlap each other, on 81 maps; 2,175 waypoints lie nearer a cliff cell than their map's widest placed walker's bounding circle, on 54 maps. No `Player_<n>_Start` waypoint does. Measured on 15 October 2026 by a script that reads the maps and the INI as the import does, each structure's box turned by its angle.

## Production

Read from the released source and the install's `INIZH.big` on 15 October 2026.

- **Command sets** (`CommandSet.ini`, `CommandButton.ini`). An object names its `CommandSet`, whose numbered lines 1 to 14 name buttons. A button of `Command = UNIT_BUILD` trains its `Object` at the selected factory; one of `DOZER_CONSTRUCT` places its `Object` for the selected dozer or worker to build. `AmericaCommandCenterCommandSet` trains `AmericaVehicleDozer` at 1; `AmericaDozerCommandSet` builds `AmericaBarracks` at 3, among 11 buildings; `AmericaBarracksCommandSet` trains `AmericaInfantryRanger` at 1. A button's `TextLabel` names its text in the string table, `CONTROLBAR:ConstructAmericaDozer` for the dozer.
- **Cost and time** (`ThingTemplate::calcCostToBuild`, `calcTimeToBuild`, `Core/GameEngine/Source/Common/Thing/ThingTemplate.cpp`). The cost is the object's `BuildCost`, times the player's cost changes and handicap, 1 in a skirmish. The time in frames is `BuildTime × 30`, truncated, times the build time handicap; divided by the power penalty when the player's power falls short, between `MinLowEnergyProductionSpeed` 0.5 and `MaxLowEnergyProductionSpeed` 0.8; and, for a unit that appears at its rally point, times `MultipleFactory` once for each factory of its kind past the first, which `GameData.ini` sets to 1.0. The dozer costs 1,000 and takes 5 s, the barracks 600 and 10 s, the ranger 225 and 5 s.
- **Prerequisites** (`ProductionPrerequisite`, `GeneralsMD/Code/GameEngine/Source/Common/RTS/ProductionPrerequisite.cpp`). Each `Object =` line of an object's `Prerequisites` must be met, and the objects of one line are alternatives, any one of which meets it; a `Science =` line needs a science. Of the install's prerequisite lines, 598 name one object, 4 name several, and 35 a science. The ranger needs `AmericaBarracks`; the barracks' one line, `AmericaPowerPlant`, is commented out.
- **Factories** (`ProductionUpdate`, `DefaultProductionExitUpdate`, `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Update/ProductionUpdate.cpp`). A factory's queue holds `MaxQueueEntries`, 9 by default. A trained unit appears at the factory's `UnitCreatePoint`, in the factory's frame, and moves to its `NaturalRallyPoint`, or to the rally point the player set: `X:-18 Y:35` and `X:60 Y:35` for the command center, `X:0 Y:0` and `X:55 Y:0` for the barracks. The cost is paid as the unit is queued, and a cancel returns it.
- **Construction** (`DozerAIUpdate`, `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Update/AIUpdate/DozerAIUpdate.cpp`). `construct` checks the player can make the object and the place is legal (terrain, a clear path, no overlap, revealed by the shroud), withdraws the whole cost, spawns the object at once, under construction, at 0 % and 1 hit point, turned at the angle the player placed it at, flattens the terrain under it, and only then gives the dozer its task. The dozer drives to the free spot nearest a point half the building's major radius from its center toward the dozer (`findGoodBuildOrRepairPosition`); there each frame adds `100 / frames` to the construction percent and `max / frames` to the health, `frames` the build time. A cancel of a construction returns the whole cost (`onDozerCancelConstruct`, `Core/GameEngine/Source/GameLogic/System/GameLogicDispatch.cpp`). The cursor's building shows at its template's `PlacementViewAngle`, −135° for America's buildings, until the player drags an angle (`InGameUI::placeBuildAvailable`).
- **Starting money** (`multiplayer.ini`, `MultiplayerSettings::addStartingMoneyChoice`). The choices are 5,000, 10,000, 20,000 and 50,000; 10,000 is marked `Default`, and the game asserts that exactly one is. `GameLogic::startNewGame` gives every player the starting cash, whether its map has a start for it or not.
- **The string table** (`GameTextManager::parseCSF`, `Core/GameEngine/Source/GameClient/GameText.cpp`). `Data\<language>\generals.csf`: a header of the tag `CSF `, its version, the counts of labels and strings, and two more integers; then each label, the tag `LBL `, its count of strings, its name's length and bytes; each string the tag `STR ` or `STRW`, its length in UTF-16 units and those units each inverted bit by bit, and for `STRW` a length and the bytes of its speech file. The game keeps a label's first string, up to its first NUL, as `stripSpaces` leaves it: a run of spaces as one, and no space at its start, at its end, or beside a newline or a tab. Labels are found ignoring case (`compareLUT`). An `&` in a button's text marks its hotkey.

## Combat

Read from the released source and the install's `INIZH.big` on 15 October 2026.

- **Life** (`ActiveBody`, `StructureBody`). An object's `Body` module holds its `MaxHealth` and `InitialHealth`. Of the install's body modules, 1,166 are `ActiveBody`, 314 `StructureBody`, 36 `ImmortalBody`, 15 `HighlanderBody`, 11 `HiveStructureBody`, 6 `UndeadBody` and 6 `InactiveBody`. The ranger has 180, the dozer 250, America's command center 5,000.
- **Weapons** (`Weapon.ini`, `WeaponTemplate`, `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Weapon.cpp`). The install defines 363 weapons. An object's `WeaponSet` blocks name a `PRIMARY`, `SECONDARY` or `TERTIARY` weapon for each set of conditions; the set of `Conditions = None` is the one it starts with. A weapon gives `PrimaryDamage`, `PrimaryDamageRadius`, `AttackRange`, `DamageType`, `DeathType`, `WeaponSpeed`, `ProjectileObject`, `DelayBetweenShots`, `ClipSize` and `ClipReloadTime`, times in ms read as `parseDurationUnsignedInt` reads them. A weapon of no projectile deals its damage `distance / WeaponSpeed` frames after the shot, the speed read per second and turned per frame by `parseVelocityReal`, the distance from the attacker's center to the target's in 3D: below one frame at once, else held by `WeaponStore::setDelayedDamage` for the frames rounded up, and from a laser, a weapon of a `LaserName`, always at once (`WeaponTemplate::fireWeaponTemplate`); the default speed is 999,999 units a second, "effectively instant". A `PrimaryDamageRadius` or a `SecondaryDamageRadius` above 0 damages every object within the larger, and a `ScatterRadius`, or a `ScatterRadiusVsInfantry` at infantry, aims at a random point within it, which may miss (`dealDamageInternal`). Of the install's starting sets, 7 weapons of no projectile set a lower speed, `HumveeGun` and `SentryDroneGun` among them. `AntiGround = No` keeps a weapon off what stands on the ground, as the anti-air `GattlingTankGunAir` and `QuadCannonGunAir` are, the default being ground alone; `RadiusDamageAffects` of `SUICIDE` kills the weapon's own unit, and of no radius it then damages nothing else (`dealDamageInternal`); `DamageDealtAtSelfPosition` deals the damage at the attacker's place, of no radius to nothing; `ContinuousFireOne` and `ContinuousFireTwo` count the shots after which the weapon fires at the faster rates of the continuous fire bonuses, as `GattlingTankGun` and `Infa_MiniGunnerGun` do. `IsHealthDamagingDamage` (`Damage.h`) holds the types that take no life: `STATUS`, `SUBDUAL_MISSILE`, `SUBDUAL_VEHICLE`, `SUBDUAL_BUILDING`, `SUBDUAL_UNRESISTABLE`, `KILL_PILOT` and `KILL_GARRISONED`; `ActiveBody::attemptDamage` gives a target of `HEALING` life. Of the install's starting sets, `AvengerTargetDesignator`, `ECMTankVehicleDisabler` and the microwave tank's `MicrowaveTankBuildingDisabler` and `MicrowaveTankBuildingClearer` deal those types. A weapon fires a shot, waits `DelayBetweenShots`, and after the last shot of its clip waits `ClipReloadTime` in its place (`Weapon::reloadWithBonus`, `onWeaponBonusChange`); a `ClipSize` of 0 is a clip without end. `RangerAdvancedCombatRifle`: 5 damage, radius 0, range 100, `SMALL_ARMS`, no projectile, 100 ms between shots, a clip of 3, 700 ms to reload. The dozer's `DozerMineDisarmingWeapon` fires only at mines; the command center has no weapon. A target is in range when the 2D distance from the edge of the attacker's bounding circle to the edge of the target's is at most `AttackRange`, and at least `MinimumAttackRange` (`Weapon::isWithinAttackRange`, `ATTACK_RANGE_CALC_TYPE` `FROM_BOUNDINGSPHERE_2D`); a box's bounding circle holds its corners, so for the command center's box of 120 by 140 its radius is √(60² + 70²), about 92.2.
- **Tank shells** (`DumbProjectileBehavior`, `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Behavior/DumbProjectileBehavior.cpp`; `BezierSegment`, `BezFwdIterator`, `Core/GameEngine/Source/Common/Bezier/`). Read on 16 October 2026. `CrusaderTankGun` and `BattleMasterTankGun`: 60 damage, `ScorpionTankGun` 20; each `ARMOR_PIERCING`, range 150, `PrimaryDamageRadius` 5, `ScatterRadiusVsInfantry` 10, `WeaponSpeed` 400, `RadiusDamageAffects = ALLIES ENEMIES NEUTRALS`, `ProjectileCollidesWith = STRUCTURES WALLS`, pitch from −15 to 15; 2,000 ms between shots, the Scorpion's 1,000. Their `ProjectileObject`s, `GenericTankShell`, `BattleMasterTankShell` and `ScorpionTankShell`, are spheres of radius 1 with a `DumbProjectileBehavior` of `FirstHeight = 10`, `SecondHeight = 10`, `FirstPercentIndent = 50%`, `SecondPercentIndent = 90%` and `FlightPathAdjustDistPerSecond = 100`; the Scorpion's adds death weapons that upgrades turn on. A shot spawns the shell at the barrel (`Weapon::positionProjectileForLaunch`), and the shell flies a cubic Bézier curve: its first point the launch, its last the target's geometric center, the two between at 50 % and 90 % of the line in x and y, and in z at 10 above the highest of the terrain along the line, the start and the end (`calcFlightPath`, `estimateTerrainExtremesAlongLine`). Its point count is ⌈length ÷ speed⌉, the length by `getApproximateLength`: half the chord plus half the control polygon, split in halves until the two differ by at most 1, and the speed `WeaponSpeed` a frame, 400 ÷ 30. `getSegmentPoints(n)` gives the points at t = i ÷ (n − 1) by forward differences in `f32`. Each frame the shell takes the next point, and detonates in the frame after its last, or after its 10 s life; each frame first moves the curve's end toward the victim's center, by at most `FlightPathAdjustDistPerSecond` a second, 100 ÷ 30 a frame, with the same point count, when the victim moved more than √0.1. A shot at infantry with a `ScatterRadiusVsInfantry` aims at a random point within that radius on the ground, follows no victim, and may miss (`fireWeaponTemplate`). The shell detonates on contact with its victim or, by `ProjectileCollidesWith`, a structure or a wall, as the partition manager tests its sphere against their geometry after the frame's updates (`PartitionContactList::processContactList`, `shouldProjectileCollideWith`); never with its launcher. A detonation deals the weapon's damage at the shell's point to each object whose bounding sphere lies within 5 of it, strictly, in 3D (`dealDamageInternal`, `DAMAGE_RANGE_CALC_TYPE` `FROM_BOUNDINGSPHERE_3D`, `distCalcProc_BoundaryAndBoundary_3D`), allies and neutrals too, but not the launcher; the damage counts as the launcher's. A tank's weapon is on a turret (`AIUpdateInterface`'s `Turret`, the Crusader's `TurretTurnRate = 180` degrees a second), which turns toward its target and fires once within 0.035 radians of it (`TurretAIAimTurretState::update`). Of the install's starting sets, 70 weapons fire a `ProjectileObject`.
- **Damage types.** Of the 363 weapons, 347 name their `DamageType`, of 32 types, ignoring case, as `INI::scanIndexList` reads them: `EXPLOSION` 141, `POISON` 35, `SMALL_ARMS` 34, `ARMOR_PIERCING` 21, `INFANTRY_MISSILE` 16, `FLAME` 15, and 26 more; a weapon that names none is of `EXPLOSION` (`WeaponTemplate::reset`).
- **Armor** (`Armor.ini`, `ArmorTemplate`, `GeneralsMD/Code/GameEngine/Source/GameLogic/Object/Armor.cpp`). The install defines 53 armors. An object's `ArmorSet` blocks name an armor for each set of conditions, as weapon sets do. An armor's lines give a damage type and a percentage; `DEFAULT` sets every type at once, and a type no line names keeps 100 %. `adjustDamage` multiplies the damage by its type's coefficient and takes a negative result as 0; `UNRESISTABLE` and `SUBDUAL_UNRESISTABLE` pass whole. `HumanArmor` takes `SMALL_ARMS` at 100 %, `DozerArmor` at 25 %, `StructureArmorTough`, America's command center's, at 50 %.
- **Defeat** (`VictoryConditions`, `GeneralsMD/Code/GameEngine/Source/GameLogic/ScriptEngine/VictoryConditions.cpp`). `GameLogic::startNewGame` sets the condition `VICTORY_NOBUILDINGS`: a player is defeated when no object of its own that is not effectively dead or destroyed is of both `KindOf` `STRUCTURE` and `MP_COUNT_FOR_VICTORY` (`Team::hasAnyBuildings`), as America's command center is. A defeated player's other objects are killed (`Player::killPlayer`). When no two undefeated players are of different alliances, the first undefeated player's alliance wins, its defeated allies among it.

## Replays

`GENREP`, then the start and end times, the frame count, whether it desynced or quit early, the players' disconnects, the replay's name, the time, the version strings and number, the CRCs of the executable and the INI, the game options as text, and the local player's slot (`RecorderClass::readReplayHeader`, `GeneralsMD/Code/GameEngine/Source/Common/Recorder.cpp`); the commands of each frame follow.

## Randomness

The logic draws from one generator: six `u32` words, each draw an add-with-carry pass over them and an increment, seeded by `InitRandom(seed)` (`Core/GameEngine/Source/Common/RandomValue.cpp`). The client and the audio draw from their own.

## Options for the importer's output

| Output | From | How | Crates |
| --- | --- | --- | --- |
| Models, `.glb` | W3D | glTF's JSON through `serde_json`, which the workspace has, and the binary container, a header and two chunks, written by hand | None new; [`gltf-json`](https://docs.rs/crate/gltf/latest) would give the JSON types |
| Textures, KTX2 | DDS | The DXT blocks and their mips copied into a KTX2 container as BC1, BC2 and BC3, with no new compression | None for the container, a header, a level index and a format descriptor; [`ruzstd`](https://docs.rs/ruzstd) for zstd, whose encoder has its fastest level done |
| Textures, PNG | TGA | Pixels into PNG | `png` |
| Sounds | WAV, MP3 | Not measured yet | |

Each is pure Rust, with no float math in what it writes, so one version and one setting should give the same bytes on every OS; the import's check on two OSes confirms it.

## Open

- **Sounds.** The WAV encodings, and an encoder of Ogg Vorbis that gives the same bytes on every OS.
- **What the INI names.** Of the 6,759 model and animation names the INI gives, 80 are in no archive, most of them editor tiles (`aiblocktile`).
