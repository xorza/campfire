# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Roads

The slice of [Roads](docs/design/roads.md): a player sees a Zero Hour map's roads, sidewalks, tracks and markings over the terrain, as the game draws them. Its four batches are in; its checks are that document's. The release game's segment of no type is D6.

### D5. A road lies on the ground

Touches the import's road meshes and the Zero Hour check, as a player's report showed a road hanging in the air ([research](docs/research/zero-hour-roads.md#on-a-slope)); it stands before the slice's end, whose look it changes.

- [ ] `road_mesh/mod.rs` lifts each column of a piece to the highest cell corner across it, as the game's `loadFloat4PtSection` does, so a `DirtRoad` junction 52 units wide on Tournament Desert's slope hangs 9.4 units above its low side; `roads.md`, A decal lies on the ground, splits each piece along the height grid's cell edges and each cell's diagonal and lays each vertex at the ground's height plus an eighth of a height step. The split goes before `road_decals`' split at the texture's repeats, and a cell's diagonal is the one its ground quad takes. Direction: what can be forgotten is a type — the ground's triangles are the one shape both the terrain and its decals take. Checks: the import's tests lay a piece across a slope to hand-computed vertices; the Zero Hour check fails on a decal vertex above the ground by more than the float, and its golden counts move.

### D6. A segment of no type is the release game's

Touches the import's road types and map roads, as the user chose the release game's way for the segments of no type; it stands with D5, before the slice's end.

- [ ] `map_roads/mod.rs` drops a segment whose template names no type, as a loss of cause `NoType`; `roads.md`, A road's type is `Roads.ini`'s, gives it the type of id 1, `TwoLane`, 8 units wide and 1 of its texture across. `RoadsIni` gains the game's ids, roads and bridges counted from 1 in their read order, and `RoadDropCause::NoType` goes, as nothing drops a segment for its type. Direction: a name is resolved once, at the load. Checks: the import's tests chain a segment of no type to a `TwoLane` segment it meets, at 8 units; the Zero Hour check's road losses lose their `NoType` count.

### End of the slice

- [ ] A player sees Tournament Desert's roads in the client as the game draws them; then the slice's decisions fold into the documents of their systems, `09-zero-hour.md` and `02-engine-core.md`, the code's citations of `roads.md` with them, and `roads.md` goes.

## Effects

The slice of [Effects](docs/design/effects.md): a player of a Zero Hour skirmish sees each shot's fire effect at its barrel, each projectile's exhaust, each hit's burst and each death's explosion, as the game's FX lists and particle systems draw them. It starts once Roads ends. Its facts are [Zero Hour's effects](docs/research/zero-hour-effects.md); its checks are that document's.

### E1. The effects format and its import

Touches the client files and the import, as every later batch reads the format; it adds nothing a player sees.

- [ ] The client files hold no effect: `campfire-client-files` gains an effects file of its step's length, its cap of particles, and particle systems and effect lists by name, each system of a rank of priority or none, as `effects.md`, Effects are a format of the engine, A system steps as its file says, and A cap by priority, from the file, give them, its emission volumes and velocities, its particles' life, size, alpha and color keys, forces, blend and texture, and its slave and attached systems; and a list's systems with their count, offset, radius, height, delay and orientation. Direction: no data in strings, as each kind is an enum and each name resolves once, as the file loads, which refuses one that resolves to nothing.
- [ ] The import reads neither `ParticleSystem.ini` nor `FXList.ini`: the Zero Hour import converts each template of `PARTICLE` type and each list's `ParticleSystem` nuggets to the effects file, and each particle texture to a package texture, and counts what it leaves as losses of their causes, by `effects.md`, What the slice leaves; it writes the install's step, cap and priorities as the file's, and trims, drops, orders and shares as `effects.md`, The import hands the client consistent effects, says. Direction: the importer takes a game's data as it is, and the engine reads it as it should be. Checks: the import's tests convert a system of each volume and velocity and a list of each nugget kind; the Zero Hour check pins the systems, lists, textures and losses.

### E2. Particles in the client

Touches the client alone: the runtime that steps and draws a system, which E3 starts; it stands after E1, whose format it reads.

- [ ] The client steps no particle: a particle system steps in whole steps of the file's length on the client's clock by `effects.md`, A system steps as its file says, with its slave and attached systems, under the file's cap of `effects.md`, A cap by priority, from the file, its scratch kept between frames. Direction: a pass visits what changed, and allocates nothing per particle. Checks: the client's tests step a system to hand-computed particles, as `effects.md`'s checks list them.
- [ ] The client draws no particle: each frame writes one mesh of billboards, camera-facing or ground-aligned, for each texture and blend, through the client's material of the files' `Blend`, by `effects.md`, Particles run on the CPU and draw as billboards of the one material. Direction: one rendering pipeline for every game. Checks: the client's tests build a frame's quads of two systems of two blends to hand-computed corners, colors and texture coordinates.

### E3. Shots, hits and deaths play their effects

Touches the import and the client files, for what names an effect, and the client, for what plays one; it ends the slice's work, as it makes the effects show in a match.

- [ ] Nothing names an effect: the import writes each weapon's `FireFX`, `ProjectileDetonationFX` and `ProjectileExhaust` beside its action's look, each `FXListDie`'s `DeathFX` of no condition beside its unit type's look, and each look state's `WeaponFireFXBone` for each weapon slot, by `effects.md`, Effects are a format of the engine, and A shot's effect at its barrel. Checks: the Zero Hour check pins the named effects.
- [ ] The client plays no effect: it plays a weapon's fire effect at the tick it derives the shot, at its state's bone of the model as drawn, its hit effect where a projectile it held ends or at the target of a shot of no projectile, its trail on the projectile while it flies, and a unit type's death effect as a unit gains `Dead`, by `effects.md`, An effect is the client's, from the state it holds. Direction: a pass visits what changed. Checks: the client's tests play each, as `effects.md`'s checks list them.

### End of the slice

- [ ] A player sees Tournament Desert's shots, hits, trails and deaths in the client as the game draws them; then the slice's decisions fold into the documents of their systems, `02-engine-core.md` for the format and the client's particles and `09-zero-hour.md` for what the import converts and loses, the code's citations of `effects.md` with them, and `effects.md` goes.

## Foundations
