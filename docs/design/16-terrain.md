# Terrain

The ground of a map, as the client draws it: one generic format over the map's height grid, drawn by the one rendering pipeline ([Direction](../../AGENTS.md#direction)), which an importer converts its game's terrain to. The first writer is Zero Hour's import ([Zero Hour](12-zero-hour.md)).

## Decisions

- **D1. A terrain is quads over the height grid, each with its own texture coordinates.** A map's terrain file, `client/maps/<name>/terrain.bin`, gives each cell of the map's `HeightGrid` a ground quad, its four corners at the cell's four samples: the texture coordinates of each corner in the terrain's texture, and the diagonal its two triangles meet on. Over the ground, overlays: each a quad of one cell, with its own texture coordinates, diagonal and alpha at each corner, drawn in the file's order over what is drawn before it. The format holds what an old tile terrain draws, Zero Hour's tiles, blends and stretched cliffs among them, as the original's renderer draws them, and a splat terrain draws as overlays of its layers; it names no rule of a game. Texture coordinates are `f32`, exact as the importer computes them: a ground quad is 33 bytes, an overlay 41, so Tournament Desert's file is 3.8 MB and the 116 shipped maps' about 0.97 GB, of 25.6 million samples and 3.0 million overlays, 11.7% of them. Coordinates of `u16` in sixteenths of a texel would halve that, 0.51 GB, and round a cliff's stretched coordinates; precision comes first, and the size is the user's disk alone.
- **D2. The terrain's texture is composed as the client loads it.** The file gives the texture's size, its count of levels, and its blocks: each a rectangle of a package texture, copied to a place of the texture at each level, a level's rectangle and place halved from the one above, and a border of its own texels around it, wrapped from its opposite edge, so a filtered or a stretched sample near a block's edge reads its own block. The client composes it once, as it reads the file: level `n` of a block from level `n` of its texture, each texture decoded from its KTX2 file by `package`'s generic decoder, of every block format the importer writes. A texture the import composed would be the same texels written to the package: 8 to 16 MB a map at 2,048 texels wide, about 1 to 2 GB over the shipped maps, where its blocks name textures the package holds already.
- **D3. The terrain is lit as every model is.** The ground and its overlays are drawn by Bevy's standard material, lit by the map's lights, `client/maps/<name>/lights.toml`, with a normal at each sample the client computes from the height grid, by the difference of its two neighbors on each axis, the map's edge clamping them. The file gives the surface's metallic, roughness and specular factors, as a material file does ([Zero Hour](12-zero-hour.md#the-client)); overlays blend by their alpha and draw one depth bias over the ground, as a layer does. No game's lighting rule enters the client.
- **D4. Rejected forms.** A glTF of each map's ground costs about 32 bytes a vertex, four vertices a cell: 14 MB for Tournament Desert, and 3.9 GB over the 116 shipped maps of 25.5 million cells, four times the file of D1. A splat terrain, layers of textures tiled across the world and blended by weight maps, as Unity's and Unreal's are, cannot hold a tile terrain: each cell of Zero Hour's draws its own tile of its class, and a cliff stretches its tile across its slope.

## Format

A `TerrainFile`, through `Binary`, checked as it reads:

- `columns`, the grid's columns, and its cells' ground quads, `(columns − 1) × (rows − 1)`, rows from the grid's first, each `uv: [[f32; 2]; 4]` at the corners in the grid's order and `flip`, whether its triangles meet on the other diagonal.
- `overlays`, each `cell`, its index, `uv`, `alpha: [u8; 4]` and `flip`, in drawing order.
- `texture`: `size: [u32; 2]`, `levels`, and `blocks`, each a package texture's path, its source rectangle in texels of its first level, its place, and its border's width.
- `surface`: `metallic`, `roughness`, `specular`.

A file whose quads are not the grid's cells, whose overlay names no cell, whose coordinates or factors are not finite, or whose block lies past the texture, does not read. A block whose texture the package lacks, or that has fewer levels than the file, leaves its rectangle black, as the ground is opaque, and the client logs it; the import reports each map's cells whose tile no block holds.

## Losses of Zero Hour's terrain

- Its own lights: the terrain is lit by the map's object lights. On 91 of the 116 shipped maps they are the terrain's own; on the other 25 a channel of a light differs by at most 0.157, and on Tournament Desert by at most 0.01.
- Its lighting per vertex and its clamp: lit per pixel, as every model is ([D13](12-zero-hour.md#decisions)).
