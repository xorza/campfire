# Campfire — Zero Hour

Command & Conquer: Generals – Zero Hour, imported from the player's own copy and played on Campfire's sim, so close to the original that no player notices a difference. The facts the import takes from are [Zero Hour research](../research/zero-hour.md).

## Decisions

- **Zero Hour is the first import.** EA released the source of Generals and Zero Hour under GPL v3 in February 2025, so every rule has a written specification, and most of the game is INI data of modules from a fixed library; Zero Hour, not the base game, as the community plays it.
- **Written from the released source, under MIT/Apache-2.0.** The importer and the rules package read the original's formulas, the order of their operations and its pathfinder closely, but the code is our own; a release that ships it takes a legal check first, and no file of the original or line copied from it enters the repository.
- **Generic modules are capabilities, Zero Hour's own are Rhai.** A module another game shares becomes a capability or an option of one; one only Zero Hour has, a general's power, its veterancy, is Rhai in the rules package; the pathfinder and the locomotors, which Rhai cannot match and a player feels in every move, become navigation backends, so the core keeps no Zero Hour word.
- **One reproducible package.** The same install, game version and importer release give the same bytes on every OS, assets included, so a session names the package by fingerprint; nobody distributes it, and each player, server and verifier makes their own.
- **Parity is read from the source, and no test plays the original.** The tests pin what the port read against cases worked by hand.
- **The presentation is data.** The client gains generic presentation systems that read data, and the importer maps Zero Hour's draw modules, effects and command sets onto them.
- **Fixtures in the tests, the install on request.** No copy of the game enters the repository or CI; the tests read small hand-written files in the original's formats, and a check on request imports the install `GENERALSZH_DATA` names, fails with none, and pins the package fingerprints a run on Linux and a run on macOS gave.
- **A random outcome is Campfire's.** The sim draws from its own PRF, so a random outcome is not the original's.
- **The skirmish AI is a server bot.** It reads what its team's clients see and sends signed orders, where the original's AI reads its logic's state, so the port can play differently.
- **A unit of Zero Hour is a meter.** Every distance keeps the original's number, so no scale rounds a value; the widest body therefore rises to 2,048 m, past Zero Hour's widest object of 1,196 units and its bridges of 1,117.
- **A rules package beside the imported mode.** The import writes `zero-hour/`, the rules package as the importer ships it, byte for byte, and `game/`, the mode that depends on it by a relative path, so the imported package names no path of the machine that made it.
- **The multiplayer rule for client objects.** Optimized trees, props and fluff, which the original spawns on the client alone and by a machine's detail settings, are client objects in every session, so the sim never holds them.
- **Night and snow draw their own models.** A look whose night or snow states draw other models gets variants, and the client draws a map's best match by the original's matching rule (`SparseMatchFinder::findBestInfoSlow`), a general feature any game may name its conditions for.
- **Thirty frames a second, the original's times.** The importer turns each time into frames by the original's rule and writes the milliseconds Campfire's rule turns into the same count, so no time moves by a frame.

## The importer

- **Files resolve as the original resolves them.** Every archive under the install in case-insensitive order of path, the first that holds a path keeping it, base Generals last.
- **Axes stay right-handed.** The original's `x` east, `y` north and `z` up are the engine's `x`, `−z` and `y`; an `f32` becomes the `Num` of its exact value.
- **Names become declared names,** and two that collide fail the import.
- **Textures stay as the game holds them.** A DDS keeps its DXT blocks and levels in KTX2; a TGA's levels are computed in integers in linear light, so every OS gives the same bytes.
- **Models become glTF.** Pivots become nodes and `z` up becomes `y` up as a rotation, so triangles keep their front; the import writes the GLB itself, so no encoder's version changes its bytes.
- **A W3D material becomes the one material.** A material is rough and of no specular, as no shader of the install turns specular on, or unlit where its gradient is off; a second pass or a detail stage becomes a layer with a blend and a depth bias in the material file, as glTF cannot say them. What the material cannot hold, a separate ambient color and the lighting per vertex, is lost ([research](../research/zero-hour.md#models)).
- **A missing texture draws as the game's `MissingTexture`,** as a player of the original sees it.
- **Terrain as the game's renderer draws it.** Each cell's tile is its ground quad and its blends overlays, cliffs stretched when `AdjustCliffTextures` says so, written to the generic terrain file ([Client](02-engine-core.md#client)).
- **The ground the sim reads is the logic's.** Each map's `[ground]` splits every cell on the logic's diagonal and gives the partition's 40-unit cells over the playable area ([Zero Hour projectiles](20-zero-hour-projectiles.md)).
- **Cliffs are walls.** The import grows the cliff cells by one ring, as the original's pathfinder grows them, and merges them row by row into rectangle walls on cells of 10, the original's pathfind cell, so no new map format is needed.
- **Only walkers and standing structures have bodies,** as the original's pathfinder routes only round a structure that is not mobile, not small and stands on the ground (`Pathfinder::classifyObjectFootprint`); a walker's circle is at most 25 m, the original's widest footprint.
- **Units move by the generic backends with the original's numbers,** its locomotor's speed and turn rate, until the `navigation::zero_hour` backends come.
- **A skirmish starts with the faction's start objects,** at the starts of a map the original lists for skirmish, as `WaypointMap::update` counts them.
- **Every command set's trains and builds are imported,** each with the original's cost, time, queue, exit and requirements, and its name from the install's string table.
- **The default money,** the `MultiplayerStartingMoneyChoice` marked default, 10,000.
- **Eight teams of one.** A skirmish is every player alone against every other, up to eight; a start is placed only for a slot a player or a bot holds, as the original places none for an empty slot.
- **Life is the original's.** An object of an active or structure body has life of its maximum and start; a site starts at 1 life, as the original's does.
- **A weapon converts when it hits one target at once.** A weapon of the starting set becomes an attack with its damage, kind, range, period and clip; one the engine cannot hold is dropped and reported by cause.
- **Defeat by the last building that counts.** A player is defeated when its last object of `STRUCTURE` and `MP_COUNT_FOR_VICTORY` dies, and its other units are killed, as `Player::killPlayer` does; the last team standing wins.

## The rules package

- **No file of the game.** `source/packages/zero-hour/` holds the Rhai of the modules only Zero Hour has, the modes' setup and end, and the presentation scripts; it ships with the importer.

## Losses

What the import does not hold against the original:

- **Driving.** A unit reaches its speed and stops at once, and turns in place before it drives, where the original accelerates, brakes and turns while it drives.
- **Paths.** A walker may stand where its center lies its radius from every wall, where the original needs whole cells clear; a fence blocks nothing, where the original routes round its line.
- **The start.** The command center stands unturned, and a starting unit where collision leaves it.
- **The widest walkers** collide as circles of 25 m.
- **Range.** Measured from each body's edge, a box's from its box, where the original measures from bounding circles, which hold a box's corners.
- **Weapon choice.** A unit attacks with its first weapon whose targeting selects the target, where the original picks the best for each target.
- **Dropped weapons.** Weapons that fire a missile, damage a second radius, splash or scatter with no shell, spare their own kind or the airborne, detonate on shrubbery or projectiles, or need a pre-attack delay, a minimum range or a random delay are not imported until the engine holds them ([Zero Hour projectiles](20-zero-hour-projectiles.md)).
- **Auto attack.** A unit attacks only what it is ordered to.
- **Weapon sets and bonuses.** Only the set of `Conditions = None`; no upgrade or veterancy changes a weapon.
- **Other bodies.** Objects of an immortal, highlander, undead, hive or inactive body have no life.
- **Deaths.** A unit that dies goes at once: no death animation, rubble or death object.
- **Alliances.** Each player is its own team.
- **Power.** A building's power is not imported, so a player short of power builds at full speed.
- **The ground under a building** is not flattened as the original flattens it.
- **A site's look.** A site draws its building whole, not its scaffold.
- **The panel.** Text buttons, with no images, hotkeys, sell, upgrades or powers.
- **Dropped buttons.** Buttons of a science prerequisite, of an either-or one, trains of a unit that does not walk, and builds of no box body are not imported.
- **Terrain light.** The terrain is lit by the map's object lights, which on most shipped maps are its own, and per pixel.
- **Random outcomes** are Campfire's.

## Open questions

- **Sound.** Which WAV encodings the game holds, and an Ogg Vorbis encoder that gives the same bytes on every OS.
