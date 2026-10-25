# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Roads

The slice of [Roads](docs/design/roads.md): a player sees a Zero Hour map's roads, sidewalks, tracks and markings over the terrain, as the game draws them. Four batches, each one commit; its checks are that document's.

### D1. Straight roads (import, client files, client)

- [ ] The import reads no `Roads.ini` (`import/src/zero_hour/format/ini/`) → each road's texture, `RoadWidth` and `RoadWidthInTexture`, by its name (roads.md, A road's type is `Roads.ini`'s).
- [ ] The map's road segments draw nothing (`import/src/zero_hour/map_import/mod.rs:188`) → each start and the end after it a segment of its type, its ends' angled, tight and join flags kept; one of no type a loss of the import (roads.md, A road's type is `Roads.ini`'s).
- [ ] No piece is meshed → a segment meshed as the game floats it: its quadrilateral from its width, a column for each map cell along it, both vertices at the highest cell corner across it plus an eighth of a step, the columns the line between neighbours keeps above dropped, its texture coordinates repeating along it (roads.md, A decal floats as the game's).
- [ ] The terrain's texture holds no road texture (`import/src/zero_hour/terrain_atlas/mod.rs:52`) → each road texture a map uses a copy beside the terrain classes, the texture 4,096 wide, a border wrapped on every side, and each decal triangle split where its coordinates cross a repeat (roads.md, A decal's texture joins the terrain's).
- [ ] The terrain file holds no decal (`client-files/src/terrain_file/mod.rs:25`) → its decals, one mesh in drawing order, each vertex's position and coordinates; the client appends them to the overlay mesh after the overlays, each vertex with its nearest sample's normal (`client/src/view/terrain_meshes/mod.rs:74`) (roads.md, Decals are the terrain file's, after its overlays; roads.md, A decal is lit as the ground under it).
- [ ] The import's losses name no road → corners, junctions and joins listed as not drawn, until the batches below draw them.
- Checks: a straight segment's mesh and its split at a repeat, to hand-computed vertices; the Zero Hour check pins Tournament Desert's segments and decal triangles, `ZeroHour::OUTPUT_VERSION` raised; the client's test appends decals after overlays; in the client, Tournament Desert's sidewalk and gravel road show over its plaza's edges.

### D2. Chains and corners (import)

- [ ] Segments meet as separate quadrilaterals → each chained to one of its type that shares its point, flipped to run on, and where two meet a curve of 1.5 road widths, 0.5 when the start is tight, or a miter when the end is angled, at the curve's place in the texture (roads.md, A road is a decal the import meshes).
- Checks: a curve, a tight curve and a miter, to hand-computed vertices and coordinates; the Zero Hour check pins Tournament Desert's curves and miters.

### D3. Junctions (import)

- [ ] Where three or four ends of a type meet, their segments overlap → a tee, a Y, an H or its mirror, or a four-way, as the ends' angles choose, the segments offset to meet it, at its place in the texture (roads.md, A road is a decal the import meshes).
- Checks: each junction's pieces to hand-computed vertices; the Zero Hour check pins Tournament Desert's junctions by kind.

### D4. Joins between types (import)

- [ ] Ends of two types at one point overlap → an alpha join where an end asks for one, and the types' stacking ordered by their joins, the decals written in stacking order (roads.md, Decals are the terrain file's, after its overlays).
- Checks: an alpha join and its stacking, to hand-computed vertices and order; the Zero Hour check pins Tournament Desert's joins; the import's losses name no road; in the client, a player sees Tournament Desert's roads as the game draws them.

## Foundations
