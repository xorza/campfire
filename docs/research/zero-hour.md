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

- **Packing.** `EAR\0`, the `u32` little-endian unpacked size, then EA's RefPack (`Core/Libraries/Source/Compression/EAC/refdecode.cpp`).
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

## Textures

- **DDS.** 6,602: DXT1 3,723, DXT5 2,870, DXT3 9; all with mip levels; sides from 4 to 1,024, 256 the commonest.
- **TGA.** 740, all uncompressed (type 2): 472 of 32 bits and 268 of 24; some sides are no power of two, as 384 and 640.

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

- **`store`** reads a file whole or as a stream from its start, `InputFile::stream`; an archive of 333 MiB needs reads at an offset.
- **`package`**'s map is `map/map.toml` (`MapData`, `campfire-capabilities/src/mode/map_data.rs`): bounds, grids, walls, paths, placed units and markers, with no heightmap and no model of a unit type.
- **`client`** draws a plane and capsules (`campfire-client/src/view/mod.rs`), and builds Bevy with no glTF, PNG or KTX2 support. Design 02 says the client loads no asset from a file, so a model's bytes reach Bevy through `store`, not Bevy's asset server.

## Open

- **Sounds.** The WAV encodings, and an encoder of Ogg Vorbis that gives the same bytes on every OS.
- **What the INI names.** Of the 6,759 model and animation names the INI gives, 80 are in no archive, most of them editor tiles (`aiblocktile`).
- **The original's oracle.** The second machine, `192.168.0.5`, has GeneralsX installed, and clones of GeneralsX and GeneralsGameCode in `~/Projects`.
