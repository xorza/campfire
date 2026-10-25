# Zero Hour's roads

How Zero Hour stores and draws its roads, read on 26 October 2026: the game's code from [GeneralsX](https://github.com/fbraz3/GeneralsX) (`W3DRoadBuffer.cpp`, `TerrainRoads.cpp`, `W3DShaderManager.cpp`, `BaseHeightMap.cpp`, `MapObject.h`), its data from the install the Zero Hour check imports, and the engine from the working tree.

## In a map

- **A road is a pair of map objects.** An object flagged `FLAG_ROAD_POINT1` (`0x02`) starts a segment, and the object after it in the list, flagged `FLAG_ROAD_POINT2` (`0x04`), ends it; the start's template names its road type. An end's other flags: `FLAG_ROAD_CORNER_ANGLED` (`0x08`) makes the corner there a miter, not a curve; `FLAG_ROAD_CORNER_TIGHT` (`0x40`), read on the start alone, makes a curve's radius 0.5 road widths, not 1.5; `FLAG_ROAD_JOIN` (`0x80`) asks for an alpha join to a road of another type there. Bridges are their own flags, `0x10` and `0x20` (`W3DRoadBuffer::addMapObjects`).
- **The import skips them.** `map_import` passes over each object of a road's or a bridge's flags, and records no loss of them (`apps/import/src/zero_hour/map_import/mod.rs`).

## In the install

- **Census of the 150 maps.** 130 hold roads: 40,960 segments, 7 starts with no end after them, and 3 segments whose template is empty. The most segments are `md_gla05_end`'s, 1,811; the most road types `md_gla05`'s, 37. Of the segments' ends, by how many ends of the same type share their point: 12,689 alone, 26,548 two, 4,566 three, 546 four, 37 five, 8 six and 2 ten; 317 points hold ends of two types. Flags: 27,535 angled, 16,298 tight, 3,069 joins. Bridge ends: 241.
- **Types used.** 60 of the 63 roads: highways and lanes (`TwoLane` 1,384, `TwoLaneDark` 2,019, `FourLane` 743 and their variants), dirt and gravel roads and paths (`DirtRoad` 1,655, `GravelRoad` 1,054), sidewalks (`Sidewalk` 7,233, `Sidewalk2` to `4` about 2,700 each), cobblestones (1,186), train tracks (875), and markings and symbols (crosswalks, caution areas and strips, arrows, parking lines, words, a helipad).
- **`Roads.ini`.** 63 roads and 27 bridges; a road's fields are `Texture`, `RoadWidth`, in world units (35 or 60 for lanes), and `RoadWidthInTexture`, a share of its texture's height (0.9 or 0.95 for most, 3 for 18 symbols and markings) (`TerrainRoads.cpp`).
- **Textures.** `TRTwoLane.tga` is `Art\Textures\trtwolane.dds`, 512 texels square; `TRSidewalk.tga` is `trsidewalk.dds`, 256 square: the game reads a texture's DDS before its TGA, as the import's model textures do. The game loads each with three levels, repeated on both axes (`RoadType::loadTexture`).
- **Limits.** `GameData.ini` sets `MaxRoadSegments` 4,000, `MaxRoadTypes` 100, and for each road type the visible pieces' `MaxRoadVertex` 3,000 and `MaxRoadIndex` 5,000: a piece past them is not drawn that frame (`W3DRoadBuffer::loadRoadSegment`).

## How the game builds them

- **Steps.** `loadRoads` adds the segments, chaining each to one of the same type that shares its point and flipping it to run on; counts the ends that meet at each point; inserts the junctions where three or four meet; inserts a curve or a miter where two meet; inserts the alpha joins and orders the types' stacking; then meshes every piece.
- **Pieces and their place in the texture.** Each piece is a quadrilateral of the ground mapped onto a region of its type's texture, its offset in 512ths: a segment at `(0, 85)`, a curve at `(4, 255)` or a tight one at `(4, 425)`, a tee at `(425, 255)`, a four-way at `(425, 425)`, a Y at `(255, 226)`, an H at `(202, 364)`, an alpha join at `(106, 425)`. A segment's width is `RoadWidth × RoadWidthInTexture`; its `u` runs `U / (4 × RoadWidth)` along it, so its texture repeats along its length, and its `v` `V / (4 × RoadWidth)` across.
- **The mesh floats.** `loadFloat4PtSection` lays a piece as columns, one for each map cell along it and at least two, of two vertices across it; a column samples the ground at each map cell across it, takes the highest corner of each sample's cell (`getMaxCellHeight`), and stands both its vertices at the highest of those plus `MAP_HEIGHT_SCALE / 8`; a column is dropped when the line between its neighbours stays above it within `1.1 × MAP_HEIGHT_SCALE`.

## How the game draws them

- **After the terrain, in stacking order.** Each road type is one draw, the types by their stacking, which the alpha joins set; every frame keeps the pieces whose ends' box meets the view (`drawRoads`, `visibilityChanged`).
- **Blended, unwritten to depth.** The texture modulates the vertex's colour, the terrain's static light at the sample nearest the vertex (`updateSegLighting`), its alpha the texture's; the result blends by source alpha over the terrain, with a depth test of less or equal and no depth write, and the cloud and noise passes over it (`RoadShader2Stage::set`).

## The engine

- **Bevy orders transparent draws by place.** The transparent phase sorts each mesh by its bounds' centre in the world plus its material's depth bias (`bevy_pbr` 0.20, `material.rs`), so two meshes over the same ground draw in an order that moves with the camera; the triangles of one mesh draw in their order.
- **The terrain's texture is one composed image.** The terrain file's texture is copies of package textures and border wraps, 2,048 texels wide for Zero Hour's layout and at most 8,192 a side; the terrain's overlays are one mesh blended over the ground in the file's order (`client-files/src/terrain_file`, `apps/client/src/view/terrain_view.rs`).
- **Hard terrain edges the roads cover.** On Tournament Desert, 600 of the 54,362 edges between cells of different textures carry no blend on either side; the sidewalk and gravel road around its central plaza lie over such edges.
