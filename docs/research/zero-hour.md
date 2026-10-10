# Zero Hour — research

What an import of Command & Conquer: Generals – Zero Hour reads, measured on a Steam install and read from the released source, for [design 12](../design/12-zero-hour.md). Facts only: what the design takes from them is design 12's. Measured on 10 October 2026; the source is [TheSuperHackers/GeneralsGameCode](https://github.com/TheSuperHackers/GeneralsGameCode) at `ab568af4` (9 October 2026), which every path below names.

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
- **Their objects.** 281,480: 82,397 ends of roads and bridges, 25,463 waypoints, and 173,620 others.
  - **Waypoints.** Ids 1 to 909, none twice on a map; every name ASCII, none a number; one name on several waypoints of a map on 66 maps.
  - **Heights.** None past the range the game drops at −1,000 and 1,593.75. 6,261 of the others stand off the ground, from −89 to 1,350, −4 at the median, most of them props sunk into it.
  - **Angles.** 562 lie past (−π, π], up to 4.73, which the game's `normalizeAngle` brings back by a turn.
  - **Places.** 232, all on campaign maps, lie past their heightmap's extent.
  - **Names.** 1,439 templates, of which none differ only in case; one non-ASCII name, on a road's end.
- **Their terrain.** Each blended tile sets exactly one of its four directions; its inverted byte holds 0 to 3, the inverted and the flipped bit. No map has an edge class. 13 maps hold no cliff UVs, and 0, which names none, in every cell.

## Terrain

`BlendTileData` (`WorldHeightMap::ParseBlendTileData`) holds, for every cell, a tile, a blend tile, a third blend tile and a cliff UV index, each an `i16`, and a bit for each cell's cliff state. Then the texture classes, each its first tile, its count, its width in tiles and its name, which `Terrain.ini` maps to a TGA of `Terrain.big`, cut into tiles; the edge classes; and the blended tiles, each the tile it blends and its direction: horizontal, vertical, either diagonal, inverted, long diagonal, and a custom edge class.

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
- **Lists of a pass.** A one-id list of vertex materials, shaders or textures is one for all (`read_vertex_material_ids`, `read_shader_ids`, `read_texture_ids`). A triangle's texture id `0xFFFFFFFF` leaves it untextured. A second chunk of a list in one pass or stage goes to the alternate material description, which a draw's default state does not show. Some texture stages hold texture ids and no coordinates.
- **Vertex colors.** `MeshMatDescClass::Post_Load_Process` moves a pass's vertex colors to the light its vertex materials use: with diffuse light, and ambient or not, they are multiplied by the diffuse color and the opacity and become the diffuse and ambient color; with ambient alone, the ambient; with emissive alone, the diffuse, with lighting off. All 69 meshes of converted models with vertex colors use diffuse and ambient light.
- **Materials.** Of the vertex materials of converted models, 336 have an ambient color other than their diffuse one. 275 meshes of converted models have a second material pass; none has no pass.
- **Texture coordinates.** 35 meshes of converted models hold `NaN` coordinates at 83 vertices, all on textured triangles; the GPU interpolates `NaN` over each triangle that touches one, so what the game shows depends on the hardware.
- **Missing textures.** A texture no archive holds draws as `MissingTexture`, a solid color of `0x7FFF00FF`. Converted models name one, `dirtground1.bmp`, in 22 meshes of one file.
- **Normals.** 619 vertices in 74 meshes of converted models have a normal of zero; 291 of them lie on a triangle of area. The game lights a vertex per vertex, so such a vertex takes ambient and emissive light alone.
- **Hide and show.** `doHideShowBoneSubObjs` walks each sub-object's bone up to the root: a change to a sub-object on the root spreads to every sub-object off the root.

## Textures

- **DDS.** 6,602: DXT1 3,723, DXT5 2,870, DXT3 9; each with its full chain of levels, its sides multiples of 4, from 4 to 1,024, 256 the commonest, 523 not square; no cube map or volume; each file's bytes exactly its header and its levels.
- **TGA.** 740, all uncompressed true color (type 2) with no color map: 472 of 32 bits and 268 of 24; every one stored bottom row first, and ending with a TGA 2.0 footer that names no extension area. 201 of those of 32 bits say 0 alpha bits in their header; Zero Hour's loader reads every TGA of 32 bits as `A8R8G8B8`, alpha in use (`Get_WW3D_Format`, `Core/Libraries/Source/WWVegas/WW3D2/ww3dformat.cpp`). 36, all terrain textures, have sides of 384 or 640, no power of two. The loader makes a TGA's levels by `BitmapHandlerClass::Combine_A8R8G8B8`, which shifts each of four texels right by 2 before it sums them, so each level darkens by up to 3 of 255.
- **Names.** No `.dds` and `.tga` share a path but for the extension.

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

## What the engine lacks

- **`package`** has no model of a unit type.
- **`client`** draws a plane and capsules (`campfire-client/src/view/mod.rs`), and builds Bevy with no glTF, PNG or KTX2 support. Design 02 says the client loads no asset from a file, so a model's bytes reach Bevy through `store`, not Bevy's asset server.

## Open

- **Sounds.** The WAV encodings, and an encoder of Ogg Vorbis that gives the same bytes on every OS.
- **What the INI names.** Of the 6,759 model and animation names the INI gives, 80 are in no archive, most of them editor tiles (`aiblocktile`).
- **The original's oracle.** The second machine, `192.168.0.5`, has GeneralsX installed, and clones of GeneralsX and GeneralsGameCode in `~/Projects`.
