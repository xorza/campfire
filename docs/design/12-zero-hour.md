# Campfire — Zero Hour

Command & Conquer: Generals – Zero Hour, imported from the player's own copy and played on Campfire's sim, so close to the original that no player notices a difference. It is the first imported game ([Games](04-capabilities/games.md)): the `import` app, the parity method and the client's presentation systems it needs are the engine's, and each later import reuses them; the importer module, the rules package and the oracle are Zero Hour's.

## Decisions

- **D1. Zero Hour is the first import.** EA released the source of Generals and Zero Hour under GPL v3 in February 2025, so every rule has a written specification, and most of the game is INI data: an object is a list of modules from a fixed library, `Body`, `Behavior`, `Draw` and the rest, each with its fields. Zero Hour, not the base game, as the community plays it.
- **D2. The importer and the rules package are MIT/Apache-2.0, written from the released source.** Near parity means reading the original's formulas, the order of their operations and its pathfinder closely; the code is our own, and a release that ships it takes a legal check first, since code written that close to GPL code can be held a derivative of it. No file of the original, and no line copied from it, enters the repository.
- **D3. Generic modules are capabilities; what only Zero Hour does is Rhai; native code only behind a backend interface.** A module whose mechanism another game shares, a weapon, a body, production, a container, becomes a capability or an option of one. A module only Zero Hour has, a general's power, its veterancy rules, its crush damage, is Rhai in the rules package. The pathfinder and the locomotors, which Rhai cannot match in speed and a player feels in every move, are a pathfinding backend and a movement backend of navigation ([Backends](02-engine-core.md#backends)), so the core keeps no Zero Hour word. Movement is a backend interface of its own, how a unit follows its route each tick, whose default is the local steering every mode has now: Zero Hour's acceleration, braking, turn rates and locomotor types, infantry, wheels, treads, hover and air, stay inside its backend, quirks included, and no other game inherits them.
- **D4. One reproducible package.** The importer writes one package from an install, the game's data, its assets and every map, each map under `map/<name>/`, which a session names and alone reads ([Packages](05-protocol-spec.md#packages)), and the same install, game version and importer release give the same bytes on every OS, assets included: the session names it by fingerprint, as it names any package. So every encoder the importer runs, of models, textures and sounds, gives the same bytes on every OS, and an importer release that converts anything differently makes a new fingerprint. Nobody distributes an imported package: each player, each server and each verifier makes their own ([Packages](05-protocol-spec.md#packages)).
- **D5. The original is the oracle.** A fork of [GeneralsX](https://github.com/fbraz3/GeneralsX), the community's build of the released source for Linux x86-64 and macOS on Apple Silicon, gains a headless player of replay files that writes each logic frame's units and events. Campfire plays the same replays, imported as inputs, and a test compares the two. The measure of "no player notices" is set from the first measurements, not before them.
- **D6. The client's presentation is data.** The client gains generic presentation systems that read data: a model and an animation for each condition state of a unit, effect lists, particle systems, terrain, and the layout of the control bar. The importer maps Zero Hour's `Draw` modules, FX lists, particle systems, command sets and window files onto them; presentation scripts cover the rest ([Who runs what](03-game-scripting.md#who-runs-what)).
- **D7. Fixtures in the tests, the install on request.** No copy of the game enters the repository or CI. The tests read small files written by hand in the original's formats, a `.big` archive, a map and a W3D model, committed with the importer. A check outside the tests, as `lan-check` is, runs on request against the install `GENERALSZH_DATA` names, and fails when it names none, so a run with no copy never passes as one that checked; it imports, compares the fingerprint with the one the other OS gave, and plays the parity suite.
- **D8. The parity test replays the original's draws.** Zero Hour draws from one generator that its replay seeds; Campfire draws from its PRF, keyed by a seed the seed chain gives ([Determinism Core](09-determinism-core.md)), so the two never draw the same values, and a match drifts from its first draw. The oracle writes every value it draws, in order; in the parity test alone, the sim takes those values in place of its PRF, through a source in its internals, and a session always draws from the PRF. A draw the port makes at another moment, or in another order, than the original fails the test as any other difference does, so the port keeps the original's draws where they are.
- **D9. The skirmish AI is a server bot.** The AI runs outside the sim, in the server, as every bot does: it reads what its team's clients see, and sends orders that the server signs and logs ([Sessions](10-sessions.md#decisions), D4). The original's AI runs inside its logic and reads its state directly, so the port can play differently. The parity test checks the world, not the AI: the oracle writes the commands the original's AI players give, and the test gives them to Campfire's bot slots as bot inputs. The AI's own play is not part of the parity measure.
- **D10. Code bound to Zero Hour lives in `zero_hour` modules.** Its importer, its pathfinding and movement backends and its AI each sit in a module named `zero_hour` of the crate whose interface they serve, so no reader takes one for a general system ([Modules](02-engine-core.md#modules)).
- **D11. A unit of Zero Hour is a meter.** Every distance, speed and size keeps the original's number, so no scale rounds a value or moves a bit of its fraction. The widest body Campfire allows rises from 64 m to 2,048 m, the power of two past Zero Hour's widest, a 1,196-unit object of the campaign, and its bridges of 1,117: the exact collision and box math, whose products the 64 m bound kept within `u128`, takes wider integers or fewer bits of its root's fraction, as the plan measures. Each body grid still sizes its cells from the widest body present, so the bound costs nothing until a wide body stands.

## The importer

A module of the `import` app, `zero_hour`, which reads an install through `store` and writes one package. What it reads, measured on an install and read from the source: [Zero Hour research](../research/zero-hour.md).

| Original | Package |
| --- | --- |
| `.big` archives | Read in place; nothing of them is kept |
| INI: objects, weapons, locomotors, upgrades, sciences, special powers, command sets | `data/`, each module to its capability's section or to the rules package's script with its fields as params |
| `.map`: heightmap, textures, placed objects, waypoints, triggers, scripts | `map/<name>/`: `map.toml`, its heights in `heights.bin`; the terrain's tiles and blends in `client/maps/<name>/terrain.bin`; the map's scripts as data the rules package runs. Each grid is postcard, a typed struct, exact and compact |
| W3D models, hierarchies and animations | glTF |
| Textures | KTX2, no supercompression, sRGB: a DDS's DXT blocks and mip levels as they are, as BC1, BC2 and BC3; a TGA as RGBA8, with mip levels the importer computes in integers |
| Audio | Ogg Vorbis |
| `.csf` strings | `locale/`, one Fluent file a language |

- **Which file.** The importer resolves each path as the original does: every archive under the install, in case-insensitive order of path, the first that holds a path keeping it, base Generals' `ZH_Generals/` last, and the duplicate `Data/INI/INIZH.big` skipped.
- **Maps.** Each map the game lists, `Maps\<folder>\<folder>.map`, becomes `map/<name>/` and `client/maps/<name>/`, read as the game reads it: unpacked by its tag, `EAR` by `RefPack` and `ZL1` to `ZL9` by zlib, and its chunks of the versions the shipped maps have, any other refused.
  - **Names.** A map's folder and an object's template become declared names: ASCII letters lowercase, digits and `_` kept, every other byte `_`, and `x` before a name that starts with no letter. Two folders, or two templates, that give one name fail the import.
  - **Axes.** The original's `x` east, `y` north and `z` up are the engine's `x`, `−z` and `y`, so the axes stay right-handed with `y` up. An angle keeps its sign, as a turn from `x` towards `y` is one from `x` towards `−z`, counter-clockwise seen from above; it is normalized as the game's `normalizeAngle` does, in `f32`, and turned into degrees rounded once. Every other `f32` becomes the `Num` of its exact value, rounded once only below ½.
  - **Heights.** `heights.bin` is a `HeightGrid` of the capabilities: a sample every 10 units, a step of 0.625, its rows the file's in reverse, so the first lies at the least `z`. The terrain's cells, `client/maps/<name>/terrain.bin`, are in the same order, a `Terrain` of `package`'s `zero_hour` module, every index each cell names checked against its list. The game draws a cell's third blend when `GameData.ini`'s `Use3WayTerrainBlends` is not 0, which the shipped install sets to 1: the import keeps them, and the client follows the setting.
  - **Objects.** The game drops an object past its height range as it reads the map, and draws a road's or a bridge's ends as terrain: the import keeps neither. A waypoint becomes marker `w<id>`, tagged `waypoint`, its name in param `name`. Every other object becomes a placed unit of its template's type, on team `neutral`, with its `z` as its `height` above the ground. The map's bounds hold its heights and every point it places.
- **Textures.** Each `.dds` and `.tga` of the archives becomes `client/textures/<its path>.ktx2`, once, by the general texture module of the importer that any import uses. Every one is marked sRGB, as color textures are. A DDS keeps its DXT1, DXT3 or DXT5 blocks and its levels as they are; a TGA becomes 8-bit RGBA, rows from the top, with alpha in every texel of 32 bits whatever its header's count of alpha bits says, as Zero Hour's loader reads it, and its full chain of levels, each a box filter of the one above in linear light: each color decoded from sRGB by a table of 2⁻¹⁶ steps, which tests check exactly, the mean taken there, and the mean encoded back to the nearest code; alpha the plain mean, rounded half up; a side of `n` texels halving to `max(1, n / 2)`, each texel below an equal share of the side above, so an odd side's middle texel is shared by weight and none is dropped.
- **Versions.** The importer knows each game version it supports, by the hashes of the files it reads, and refuses any other with the version it found.
- **Times and numbers.** Zero Hour runs 30 logic frames a second, and its mode allows 30 Hz alone. The original states times in milliseconds and turns each into frames by its own rule; the importer applies that rule and writes the milliseconds that Campfire's rule, rounded up, turns into the same count, so no time moves by a frame. Each decimal becomes a `Num`, rounded once at import.
- **Replays.** A `.rep` file is the commands of each player by frame: the importer turns one into Campfire inputs, for the parity tests.

## The rules package

`source/packages/zero-hour/`, an ordinary package that holds no file of the game and ships with the importer. The imported package is the mode: its manifest, data, maps and assets; it depends on the rules package, and its mode data names the rules package's mode script, `script = { package, path }` ([Data files](08-script-api.md#data-files)).

- The table from each module type to the capability or the script that runs it, which the importer reads.
- The Rhai scripts of the modules only Zero Hour has.
- The modes: the skirmish's setup and its end.
- The presentation scripts, and the control bar's rules, without its images.

The order of the work is the module audit's: every module type of Zero Hour's INI, with the count of objects that use it, and its place, a capability, a script or a backend.

## Parity

- **The oracle** plays a replay headless and writes, each frame, every object's id, type, owner, position, health and state, each event, a shot, a hit, a death, a build, a train, a power, each value it draws, and the commands of its AI players.
- **The test** plays the same replay in Campfire and compares frame by frame: the first difference past the measure fails the test, with the frame, the object and the field.
- **The suite** is a set of community replays that covers the module types the import runs, each in its own test, as a replay is a scripted match whose script a player wrote.
- **Randomness.** The test gives the sim the oracle's draws ([D8](#decisions)), so a random outcome is the original's.

## The client

An imported package's models and textures reach Bevy through a package asset source: its reader reads through `package` and `store` and checks each file against the package's index, and Bevy's own glTF and KTX2 loaders parse the bytes, on demand, a model's textures by their paths in the same source.

A unit type's presentation is data of its package's client, `client/units.toml`: `[units.<id>]` with `model`, a `.glb` of the package, and, as the presentation systems grow, a model and an animation for each condition state; a type with none is drawn as the engine's own shapes are. The sim reads no part of it.

The presentation systems of [D6](#decisions), generic, which every imported game uses: a unit's model and animation follow its condition state, as Zero Hour's `W3DModelDraw` does; an event plays its effect list; terrain draws from its heightmap and texture blends; the control bar lays out the commands of the selected units. Floats stay in the client, as everywhere ([Bevy](02-engine-core.md#bevy)).

## Open questions

- **Sound.** Which WAV encodings the game holds, and an encoder of Ogg Vorbis that gives the same bytes on every OS. Models and textures need none: the importer writes the GLB and KTX2 containers itself, glTF's JSON through `common`'s `codec`, and copies or computes their contents in integers.
- **The measure.** Its numbers, from the first measurements of the oracle against Campfire.
- **The AI's play.** How the port's skirmish AI is compared with the original's, as the parity test leaves it out ([D9](#decisions)).
- **The oracle's own determinism.** Whether GeneralsX plays a retail replay to the same end on Linux and on macOS, which the first slice checks.
