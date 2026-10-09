# Campfire — Zero Hour

Command & Conquer: Generals – Zero Hour, imported from the player's own copy and played on Campfire's sim, so close to the original that no player notices a difference. It is the first imported game ([Games](04-capabilities/games.md)): the `import` app, the parity method and the client's presentation systems it needs are the engine's, and each later import reuses them; the importer module, the rules package and the oracle are Zero Hour's.

## Decisions

- **D1. Zero Hour is the first import.** EA released the source of Generals and Zero Hour under GPL v3 in February 2025, so every rule has a written specification, and most of the game is INI data: an object is a list of modules from a fixed library, `Body`, `Behavior`, `Draw` and the rest, each with its fields. Zero Hour, not the base game, as the community plays it.
- **D2. The importer and the rules package are MIT/Apache-2.0, written from the released source.** Near parity means reading the original's formulas, the order of their operations and its pathfinder closely; the code is our own, and a release that ships it takes a legal check first, since code written that close to GPL code can be held a derivative of it. No file of the original, and no line copied from it, enters the repository.
- **D3. Generic modules are capabilities; what only Zero Hour does is Rhai; native code only behind a backend interface.** A module whose mechanism another game shares, a weapon, a body, production, a container, becomes a capability or an option of one. A module only Zero Hour has, a general's power, its veterancy rules, its crush damage, is Rhai in the rules package. The pathfinder and the locomotors, which Rhai cannot match in speed and a player feels in every move, are a pathfinding backend and a movement backend of navigation ([Backends](02-engine-core.md#backends)), so the core keeps no Zero Hour word. Movement is a backend interface of its own, how a unit follows its route each tick, whose default is the local steering every mode has now: Zero Hour's acceleration, braking, turn rates and locomotor types, infantry, wheels, treads, hover and air, stay inside its backend, quirks included, and no other game inherits them.
- **D4. One reproducible package.** The importer writes one package from an install, and the same install, game version and importer release give the same bytes on every OS, assets included: the session names it by fingerprint, as it names any package. So every encoder the importer runs, of models, textures and sounds, gives the same bytes on every OS, and an importer release that converts anything differently makes a new fingerprint. Nobody distributes an imported package: each player, each server and each verifier makes their own ([Packages](05-protocol-spec.md#packages)).
- **D5. The original is the oracle.** A fork of [GeneralsX](https://github.com/fbraz3/GeneralsX), the community's build of the released source for Linux x86-64 and macOS on Apple Silicon, gains a headless player of replay files that writes each logic frame's units and events. Campfire plays the same replays, imported as inputs, and a test compares the two. The measure of "no player notices" is set from the first measurements, not before them.
- **D6. The client's presentation is data.** The client gains generic presentation systems that read data: a model and an animation for each condition state of a unit, effect lists, particle systems, terrain, and the layout of the control bar. The importer maps Zero Hour's `Draw` modules, FX lists, particle systems, command sets and window files onto them; presentation scripts cover the rest ([Who runs what](03-game-scripting.md#who-runs-what)).
- **D7. Fixtures in the tests, the install on request.** No copy of the game enters the repository or CI. The tests read small files written by hand in the original's formats, a `.big` archive, a map and a W3D model, committed with the importer. A check outside the tests, as `lan-check` is, runs on request against the install `GENERALSZH_DATA` names, and fails when it names none, so a run with no copy never passes as one that checked; it imports, compares the fingerprint with the one the other OS gave, and plays the parity suite.
- **D8. The parity test replays the original's draws.** Zero Hour draws from one generator that its replay seeds; Campfire draws from its PRF, keyed by a seed the seed chain gives ([Determinism Core](09-determinism-core.md)), so the two never draw the same values, and a match drifts from its first draw. The oracle writes every value it draws, in order; in the parity test alone, the sim takes those values in place of its PRF, through a source in its internals, and a session always draws from the PRF. A draw the port makes at another moment, or in another order, than the original fails the test as any other difference does, so the port keeps the original's draws where they are.
- **D9. The skirmish AI is a server bot.** The AI runs outside the sim, in the server, as every bot does: it reads what its team's clients see, and sends orders that the server signs and logs ([Sessions](10-sessions.md#decisions), D4). The original's AI runs inside its logic and reads its state directly, so the port can play differently. The parity test checks the world, not the AI: the oracle writes the commands the original's AI players give, and the test gives them to Campfire's bot slots as bot inputs. The AI's own play is not part of the parity measure.
- **D10. Code bound to Zero Hour lives in `zero_hour` modules.** Its importer, its pathfinding and movement backends and its AI each sit in a module named `zero_hour` of the crate whose interface they serve, so no reader takes one for a general system ([Modules](02-engine-core.md#modules)).

## The importer

A module of the `import` app, `zero_hour`, which reads an install through `store` and writes one package:

| Original | Package |
| --- | --- |
| `.big` archives | Read in place; nothing of them is kept |
| INI: objects, weapons, locomotors, upgrades, sciences, special powers, command sets | `data/`, each module to its capability's section or to the rules package's script with its fields as params |
| `.map`: heightmap, textures, placed objects, waypoints, triggers, scripts | `map/`, and the map's scripts as data the rules package runs |
| W3D models, hierarchies and animations | glTF |
| Textures | KTX2 + zstd, or PNG |
| Audio | Ogg Vorbis |
| `.csf` strings | `locale/`, one Fluent file a language |

- **Versions.** The importer knows each game version it supports, by the hashes of the files it reads, and refuses any other with the version it found.
- **Times and numbers.** Zero Hour runs 30 logic frames a second, and its mode allows 30 Hz alone. The original states times in milliseconds and turns each into frames by its own rule; the importer applies that rule and writes the milliseconds that Campfire's rule, rounded up, turns into the same count, so no time moves by a frame. Each decimal becomes a `Num`, rounded once at import.
- **Replays.** A `.rep` file is the commands of each player by frame: the importer turns one into Campfire inputs, for the parity tests.

## The rules package

`source/packages/zero-hour/`, an ordinary package that holds no file of the game and ships with the importer: the imported package depends on it.

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

The presentation systems of [D6](#decisions), generic, which every imported game uses: a unit's model and animation follow its condition state, as Zero Hour's `W3DModelDraw` does; an event plays its effect list; terrain draws from its heightmap and texture blends; the control bar lays out the commands of the selected units. Floats stay in the client, as everywhere ([Bevy](02-engine-core.md#bevy)).

## Open questions

- **Reproducible encoders.** Which encoders of glTF, KTX2 and Ogg Vorbis give the same bytes on every OS, and whether one written in Rust is needed.
- **The packages of a game.** Whether one package holds the whole game with its maps, or each map is a mode package that depends on the game's; a package holds at most 16,384 files and 16 MiB of parsed files ([Packages](05-protocol-spec.md#packages)), which the game's models, textures and INI may pass.
- **The measure.** Its numbers, from the first measurements of the oracle against Campfire.
- **The scale.** How Zero Hour's units become meters: a power of two keeps every value exact in a `Num`, and the largest bodies must stay within the widest body Campfire allows, 64 m ([Space and map](04-capabilities/00-overview.md#space-and-map)).
- **The AI's play.** How the port's skirmish AI is compared with the original's, as the parity test leaves it out ([D9](#decisions)).
- **The oracle's own determinism.** Whether GeneralsX plays a retail replay to the same end on Linux and on macOS, which the first slice checks.
