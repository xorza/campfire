# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## 1. The terrain on the one pipeline

`package`: `client_data`, `zero_hour`; `client`: `view`, `zero_hour`; `import`: `zero_hour`; `checks/zero-hour-check`. One commit, as the format's change crosses its writer, its reader and its check.

**Slice 4, the terrain drawn by the one pipeline.** The user opens Tournament Desert in the client and sees its terrain as now, its tiles, blends and stretched cliffs, from the generic terrain file, lit by the map's lights as the models are; the client and the package hold no Zero Hour code. The design is [Terrain](docs/design/16-terrain.md), and design 12's Heights, Terrain textures, Lighting and Game data. It takes the foundation "The terrain is Zero Hour's" and its Client item. The slice ends when the user compares the terrain with the original at the same camera.

Each step ends with `./drive.py check` passing: the client reads the generic file before the import writes it, and the package's `zero_hour` module goes last, with its one remaining user, the import. Between T2 and T3 the client cannot draw a package the import of that time writes; the batch is one commit, so no commit holds that state.

### T1. The generic terrain file

(`package`; Terrain D1, D2.) `client_data` gains `TerrainFile`: the ground quads, the overlays, the texture's size, levels and blocks, and the surface, checked as it reads. The KTX2 decoder of `ClassTexture` becomes a generic type of `client_data`, as the client composes any block's texture by it. Check: a file reads back what it writes; each flaw of the format's list is refused; a KTX2 of each block format the importer writes decodes to its levels.

### T2. The terrain drawn from the generic file

(`client`; Terrain D2, D3; **Client**, "The client draws an imported map's terrain by a module of Zero Hour's".) `view` gains the generic terrain: it composes the texture of the blocks at each level, with each block's wrapped border, a missing block black and logged; builds the ground mesh and one overlay mesh over the height grid, with the normals of D3 and each overlay's alpha as its vertices' color, in the file's order; and draws them by Bevy's standard material of the file's surface, lit by the map's lights, the overlays blended one depth bias over the ground. `client::zero_hour` goes, and the client reads no `client/game_data.toml`. Check: the texture a file of two blocks composes at two levels, its borders wrapped, a missing block black; the meshes of a fixture grid, each position, normal, coordinate, alpha and diagonal computed by hand.

### T3. Zero Hour's terrain converted

(`import`, `package`, the Zero Hour check; design 12.) The import converts each map's terrain to a `TerrainFile`: the client's cell rules of before T2, each tile's quarter, the blend shapes' corner alphas and diagonals, the cliff UVs, the third blends and `GameData.ini`'s two settings, and `package`'s atlas layout, as the file's blocks, move into `import::zero_hour`, with `Terrain` and its parts; `package`'s `zero_hour` module goes, and `client/game_data.toml` is no file of the package. The terrain's own lights are dropped, and the surface is rough, of no metal or specular, as D13's materials. The import's report counts each map's cells whose tile no block holds. Check: the tests of the cell rules and the atlas move with them and keep their expected values, now on the file's quads, overlays and blocks; the Zero Hour check pins Tournament Desert's quads, overlays and blocks, logs them for review, and pins its 0 untextured cells from the report; the user compares Tournament Desert's terrain in the client with the original at the same camera.

## Foundations

- **The terrain is Zero Hour's** (Client). The client's terrain is a format and a lighting rule of one game, drawn by game-bound code of the client, where every game is drawn by one rendering pipeline; each later terrain feature of the presentation, and a second imported game's terrain, meets the same limit. Items: Client, "The client draws an imported map's terrain by a module of Zero Hour's".

