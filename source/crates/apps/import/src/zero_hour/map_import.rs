use std::collections::BTreeSet;

use bevy_math::Vec3;
use campfire_client_files::HeightGrid;
use campfire_client_files::{
    ClientObject, ClientObjects, ClientUnits, MapLight, MapLights, TerrainFile, TerrainSurface,
};
use campfire_codec::{Binary, Toml};
use campfire_math::Num;
use serde::Serialize;

use crate::zero_hour::error::MapError;
use crate::zero_hour::game_data::GameData;
use crate::zero_hour::map_file::{GameLight, MapFile, MapHeights};
use crate::zero_hour::map_object::Placement;
use crate::zero_hour::object_ini::{ObjectIni, PlacedAs};
use crate::zero_hour::terrain::{Terrain, TerrainParts};
use crate::zero_hour::terrain_atlas::TerrainAtlas;
use crate::zero_hour::terrain_cells::TerrainCells;
use crate::zero_hour::unit_types::UnitTypes;

/// One map's files in the package: `map/<name>/map.toml`, `map/<name>/heights.bin`,
/// `client/maps/<name>/terrain.bin`, `client/maps/<name>/lights.toml` and
/// `client/maps/<name>/objects.toml`; and how many cells its terrain draws a tile of with no
/// texture.
#[derive(Debug)]
pub(crate) struct MapImport {
    pub(crate) map: String,
    pub(crate) heights: Vec<u8>,
    pub(crate) terrain: Vec<u8>,
    pub(crate) untextured: usize,
    pub(crate) lights: String,
    pub(crate) objects: String,
}

/// What a map import reads and adds to beside its file: the object INI, the unit types of the
/// maps so far, the looks of the package's `client/units.toml`, how many of the map's lights
/// the game draws, and the settings of `GameData.ini` its terrain is drawn by.
#[derive(Debug)]
pub(crate) struct MapSources<'a> {
    pub(crate) ini: &'a ObjectIni,
    pub(crate) types: &'a mut UnitTypes,
    pub(crate) looks: &'a ClientUnits,
    pub(crate) lights: usize,
    pub(crate) game: GameData,
}

/// The terrain's surface, rough, of no metal or specular, as the import's materials are (design
/// 12, D13).
const SURFACE: TerrainSurface = TerrainSurface {
    metallic: 0.0,
    roughness: 1.0,
    specular: 0.0,
};

/// A cell of the heightmap, 10 units wide (`MAP_XY_FACTOR`).
const CELL: i64 = 10;

/// A step of height, 0.625 units (`MAP_HEIGHT_SCALE`): 5 · 2⁻³, exact in a `Num`.
const STEP: Num = Num::from_bits(5 << (Num::FRAC_BITS - 3));

/// The team every placed unit stands on until the import reads the map's sides.
const NEUTRAL: &str = "neutral";

/// The tag of a marker made from a waypoint.
const WAYPOINT: &str = "waypoint";

#[derive(Debug, Serialize)]
struct MapToml {
    bounds: BoundsToml,
    units: Vec<UnitToml>,
    markers: Vec<MarkerToml>,
}

#[derive(Debug, Serialize)]
struct BoundsToml {
    min: [Num; 2],
    max: [Num; 2],
}

#[derive(Debug, Serialize)]
struct UnitToml {
    unit_type: String,
    team: &'static str,
    pos: [Num; 2],
    angle: Num,
    height: Num,
}

#[derive(Debug, Serialize)]
struct MarkerToml {
    name: String,
    tags: [&'static str; 1],
    pos: [Num; 2],
    params: MarkerParams,
}

#[derive(Debug, Serialize)]
struct MarkerParams {
    name: String,
}

impl MapImport {
    /// The files of `file`, of `sources`: its heights on the engine's axes, its terrain over them
    /// as a terrain file, its texture the atlas the game builds of its classes; its objects as a
    /// multiplayer game spawns them (design 12, D14), none of a template no INI object names, an
    /// optimized tree, a prop and fluff as client objects, of a look of `looks`, or none when it
    /// has none, and every other as a unit of the type `types` names, the bridge-like ones first,
    /// in the map's order; its waypoints as markers; bounds that hold its heights and every point
    /// its sim places; and its first `lights` object lights, as the game draws `NumberGlobalLights`
    /// of each, which the terrain is lit by too; their ambient color is the first light's, as the
    /// game takes it.
    pub(crate) fn new(file: &MapFile, sources: MapSources<'_>) -> Result<MapImport, MapError> {
        let MapSources {
            ini,
            types,
            looks,
            lights,
            game,
        } = sources;
        let heights = MapImport::heights(&file.heights)?;
        let map_lights = MapLights::new(
            file.lights[0].ambient,
            file.lights[..lights].iter().map(|light| MapLight {
                color: light.diffuse,
                direction: MapImport::axes(light),
            }),
        )
        .ok_or(MapError::Lighting)?;
        let terrain = Terrain::new(TerrainParts {
            cells: MapImport::flipped(&file.terrain.cells, file.heights.width),
            ..file.terrain.clone()
        })
        .map_err(MapError::Terrain)?;
        let atlas = TerrainAtlas::new(terrain.parts());
        let cells = TerrainCells::new(terrain.parts(), heights.samples(), &atlas, game);
        let untextured = cells.untextured();
        let quads = cells.quads();
        let terrain = TerrainFile::new(quads, atlas.into_texture(), SURFACE)
            .map_err(MapError::TerrainFile)?;
        let mut low = heights.origin();
        let mut high = heights.far_corner();
        let mut take = |point: [Num; 2]| {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
            point
        };
        let mut bridges = Vec::new();
        let mut units = Vec::new();
        let mut client = ClientObjects::default();
        let mut markers = Vec::new();
        let mut waypoints = BTreeSet::new();
        for (at, object) in file.objects.iter().enumerate() {
            if object.road_or_bridge() {
                continue;
            }
            if let Some(waypoint) = &object.waypoint {
                let pos = take(object.ground().ok_or(MapError::Placement(at))?);
                let id = u32::try_from(waypoint.id)
                    .ok()
                    .ok_or(MapError::WaypointId(waypoint.id))?;
                if !waypoints.insert(id) {
                    return Err(MapError::WaypointTwice(waypoint.id));
                }
                let name = String::from_utf8(waypoint.name.clone())
                    .ok()
                    .filter(|name| name.is_ascii())
                    .ok_or(MapError::NotAscii)?;
                // A marker's param that reads as a decimal is taken for a number.
                if name.parse::<Num>().is_ok() {
                    return Err(MapError::WaypointName(name));
                }
                markers.push(MarkerToml {
                    name: format!("w{id}"),
                    tags: [WAYPOINT],
                    pos,
                    params: MarkerParams { name },
                });
            } else {
                let Some(template) = ini.object(&object.template) else {
                    continue;
                };
                let Placement {
                    ground,
                    height,
                    degrees,
                } = object.placement().ok_or(MapError::Placement(at))?;
                let placed = template.placed_as();
                if matches!(placed, PlacedAs::Tree | PlacedAs::Prop) {
                    let look = template.import_name().declared();
                    if looks.objects.contains_key(&look) {
                        client.objects.push(ClientObject {
                            look,
                            pos: ground,
                            height,
                            angle: degrees,
                        });
                    }
                    continue;
                }
                let unit = UnitToml {
                    unit_type: types.name(&object.template)?.as_str().to_owned(),
                    team: NEUTRAL,
                    pos: take(ground),
                    angle: degrees,
                    height,
                };
                match placed {
                    PlacedAs::Bridge => bridges.push(unit),
                    _ => units.push(unit),
                }
            }
        }
        bridges.append(&mut units);
        let map = MapToml {
            bounds: BoundsToml {
                min: low,
                max: high,
            },
            units: bridges,
            markers,
        };
        Ok(MapImport {
            map: Toml::write(&map),
            heights: Binary::encode(&heights),
            terrain: Binary::encode(&terrain),
            untextured,
            lights: Toml::write(&map_lights),
            objects: Toml::write(&client),
        })
    }

    /// The way `light` goes on the engine's axes: the game's `x`, `y` and `z` are its `x`, `−z`
    /// and `y`.
    fn axes(light: &GameLight) -> Vec3 {
        let [x, y, z] = light.position;
        Vec3::new(x, z, -y)
    }

    /// The heights on the engine's axes. The original's sample of column `i` and row `j` lies at
    /// `x = (i − border) · 10` and its `y = (j − border) · 10`, which is the engine's `−z`: so
    /// the engine's rows run the file's in reverse, from `z = −(depth − 1 − border) · 10`.
    fn heights(heights: &MapHeights) -> Result<HeightGrid, MapError> {
        let MapHeights {
            width,
            depth,
            border,
            samples,
        } = heights;
        if border >= width || border >= depth {
            return Err(MapError::Heights);
        }
        let back = |count: usize| {
            i64::try_from(count)
                .ok()
                .and_then(|count| count.checked_mul(-CELL))
                .and_then(Num::from_int)
                .ok_or(MapError::Heights)
        };
        let origin = [back(*border)?, back(depth - 1 - border)?];
        HeightGrid::new(
            origin,
            Num::int(CELL),
            STEP,
            u32::try_from(*width).ok().ok_or(MapError::Heights)?,
            MapImport::flipped(samples, *width),
        )
        .ok_or(MapError::Heights)
    }

    /// `rows`, each `width` entries, in reverse.
    fn flipped<T: Copy>(rows: &[T], width: usize) -> Vec<T> {
        rows.rchunks_exact(width).flatten().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use campfire_client_files::ClientUnit;
    use campfire_mode::{MapData, MapPoint, MarkerData, ModeParam, PlacedUnitData};
    use campfire_values::DeclaredName;

    use super::*;
    use crate::zero_hour::map_file::internals::map;
    use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};
    use crate::zero_hour::terrain::{BlendShape, BlendTile};

    /// The import of `file` of the fixture install's objects, with the looks of its client
    /// objects but the prop of no model's, drawn in `lights` lights.
    fn import(file: &MapFile, types: &mut UnitTypes, lights: usize) -> Result<MapImport, MapError> {
        let mut ini = ObjectIni::default();
        for objects in [DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS] {
            ini.read(objects.as_bytes()).unwrap();
        }
        let objects = ["oak", "rock", "rockred", "bush"].map(|look| {
            (
                DeclaredName::new(look).unwrap(),
                ClientUnit { models: Vec::new() },
            )
        });
        let looks = ClientUnits {
            objects: objects.into(),
            ..ClientUnits::default()
        };
        let sources = MapSources {
            ini: &ini,
            types,
            looks: &looks,
            lights,
            game: GameData {
                adjust_cliff_textures: true,
                three_way_blends: true,
            },
        };
        MapImport::new(file, sources)
    }

    #[test]
    fn a_map_s_lights_are_its_time_of_day_s_on_the_engine_s_axes() {
        let mut file = MapFile::read(&map(8)).unwrap();
        let mut types = UnitTypes::default();
        let imported = import(&file, &mut types, 3).unwrap();
        // The afternoon's lights: light `l` of time 2 has ambient [2, l, 0], diffuse [2, l, 1]
        // and position [2, l, 2], which is the engine's [2, 2, −l]. The objects' lights are 3 to
        // 5, after the terrain's 0 to 2, which the import drops; their ambient is light 3's.
        let object_light = |at: f32| MapLight {
            color: [2.0, at, 1.0],
            direction: Vec3::new(2.0, 2.0, -at),
        };
        assert_eq!(
            Toml::parse::<MapLights>(&imported.lights).unwrap(),
            MapLights {
                ambient: [2.0, 3.0, 0.0],
                lights: vec![object_light(3.0), object_light(4.0), object_light(5.0)],
            }
        );
        // Drawn in two lights, each keeps its first two.
        let two = import(&file, &mut types, 2).unwrap();
        assert_eq!(
            Toml::parse::<MapLights>(&two.lights).unwrap().lights.len(),
            2
        );
        // An object light it draws that goes no way is refused; one it does not draw is not.
        file.lights[1].position = [0.0; 3];
        let refused = import(&file, &mut types, 3).map(drop);
        assert_eq!(refused, Err(MapError::Lighting));
        assert!(import(&file, &mut types, 1).is_ok());
    }

    #[test]
    fn a_map_imports_to_its_heights_units_and_markers_computed_by_hand() {
        let file = MapFile::read(&map(8)).unwrap();
        let mut types = UnitTypes::default();
        let imported = import(&file, &mut types, 3).unwrap();

        // 4 × 3 samples, a border of 1: x from −10 to 20, and z from −(3 − 1 − 1) · 10 = −10 to
        // 10, the file's last row first.
        let heights = Binary::decode::<HeightGrid>(&imported.heights).unwrap();
        let expected = HeightGrid::new(
            [Num::int(-10), Num::int(-10)],
            Num::int(10),
            Num::int(5) / Num::int(8),
            4,
            vec![8, 20, 10, 40, 16, 5, 6, 7, 0, 1, 2, 3],
        )
        .unwrap();
        assert_eq!(heights, expected);

        // The file's row 2 is the engine's row 0, so its cell of column x and row y is the
        // engine's (2 − y) · 4 + x: its last, a cliff of tile 3, the engine's 3; its first, a
        // cliff of tile 0, the engine's 8; its cell 2, of cliff UVs, the engine's 10.
        let cells = MapImport::flipped(&file.terrain.cells, file.heights.width);
        assert_eq!(cells.len(), 12);
        assert!(cells[3].cliff && cells[3].tile == 3);
        assert!(cells[8].cliff && cells[8].tile == 0);
        assert_eq!(cells[10].cliff_uv, Some(0));
        assert_eq!(cells[5].blend, Some(0));
        assert_eq!(cells[6].extra_blend, Some(0));
        assert_eq!(cells.iter().filter(|cell| cell.cliff).count(), 2);
        // The terrain file: the game draws 3 × 2 cells. Its overlays are the blend of the game's
        // cell [1, 1], the file's [1, 0], and the third blend of [2, 1], the file's [2, 0]. Its
        // classes have no texture here, so no tile copies, each block keeps its border, and every
        // cell is untextured.
        let terrain = Binary::decode::<TerrainFile>(&imported.terrain).unwrap();
        assert_eq!(
            (terrain.quads().columns, terrain.quads().ground.len()),
            (4, 6)
        );
        let overlays: Vec<u32> = terrain
            .quads()
            .overlays
            .iter()
            .map(|overlay| overlay.cell)
            .collect();
        assert_eq!(overlays, [1, 2]);
        assert!(terrain.texture().copies.is_empty());
        assert_eq!(terrain.texture().wraps.len(), 2);
        assert_eq!(terrain.surface(), SURFACE);
        assert_eq!(imported.untextured, 6);
        assert_eq!(
            file.terrain.blends,
            [BlendTile {
                tile: 4,
                shape: BlendShape::LeftDiagonal { long: true },
                inverted: true,
                flipped: true,
                custom_edge: None,
            }]
        );

        // The tank at the original's (30, 5) stands at x 30, z −5, turned π/2 in `f32`,
        // 1,509,949,482 steps of 2⁻²⁴ degrees. The rock at (−10, 15), 4.5 below the ground,
        // turned 4.729842 − 2π = −1.5533433 in `f32`, −1,493,172,475.64 steps, so
        // −1,493,172,476. The wall at (10, 0), which the map lists after both, comes first, as the
        // game spawns bridge-like objects before the others. The waypoint at (12.5, 7.5) is marker
        // `w3`. The road's end is none.
        let map: MapData = Toml::parse(&imported.map).unwrap();
        let decimal = |text: &str| text.parse::<Num>().unwrap();
        let ground = |x: &str, z: &str| MapPoint::Ground([decimal(x), decimal(z)]);
        let unit = |unit_type: &str, pos, angle: i64, height: Num| PlacedUnitData {
            unit_type: DeclaredName::new(unit_type).unwrap(),
            team: DeclaredName::new("neutral").unwrap(),
            pos,
            angle: Num::from_bits(angle),
            height,
            path: None,
            from: None,
        };
        assert_eq!(
            map.units,
            [
                unit("fortresswall", ground("10", "0"), 0, Num::ZERO),
                unit(
                    "americatankcrusader",
                    ground("30", "-5"),
                    1_509_949_482,
                    Num::ZERO
                ),
                unit(
                    "rocks1",
                    ground("-10", "-15"),
                    -1_493_172_476,
                    Num::int(-9) / Num::int(2)
                ),
            ]
        );
        assert_eq!(
            map.markers,
            [MarkerData {
                name: DeclaredName::new("w3").unwrap(),
                tags: vec![DeclaredName::new("waypoint").unwrap()],
                pos: Some(ground("12.5", "-7.5")),
                region: None,
                team: None,
                params: [(
                    DeclaredName::new("name").unwrap(),
                    ModeParam::Text("Player_1_Start".to_owned()),
                )]
                .into(),
                events: false,
            }]
        );
        // The bounds hold the heights and reach the tank's x of 30 and the rock's z of −15, and
        // not the tree's x of 50, which the sim does not hold.
        let bounds = Toml::parse("min = [-10, -15]\nmax = [30, 10]").unwrap();
        assert_eq!(map.bounds, bounds);
        // The tree, the prop 1 above the ground, its reskin and the fluff are client objects, in
        // the map's order, of their templates' looks; the prop of no look, the template no object
        // names and the one of another case are none; the fence that a build clears is a unit.
        let client = |look: &str, x: i64, z: i64, height: i64| ClientObject {
            look: DeclaredName::new(look).unwrap(),
            pos: [Num::int(x), Num::int(z)],
            height: Num::int(height),
            angle: Num::ZERO,
        };
        assert_eq!(
            Toml::parse::<ClientObjects>(&imported.objects).unwrap(),
            ClientObjects {
                objects: vec![
                    client("oak", 50, 0, 0),
                    client("rock", 5, -5, 1),
                    client("rockred", 5, 5, 0),
                    client("bush", 0, -5, 0),
                ]
            }
        );
        assert_eq!(
            types.toml(),
            "[units.americatankcrusader]\n\n[units.fortresswall]\n\n[units.rocks1]\n"
        );
    }

    #[test]
    fn a_map_whose_waypoints_or_objects_the_engine_cannot_hold_is_refused() {
        let mut file = MapFile::read(&map(8)).unwrap();
        let import = |file: &MapFile| import(file, &mut UnitTypes::default(), 3).map(drop);
        let waypoint = 2;
        let mut twice = file.clone();
        twice.objects.push(file.objects[waypoint].clone());
        assert_eq!(import(&twice), Err(MapError::WaypointTwice(3)));
        let mut numbered = file.clone();
        numbered.objects[waypoint].waypoint.as_mut().unwrap().name = b"12.5".to_vec();
        assert_eq!(
            import(&numbered),
            Err(MapError::WaypointName("12.5".to_owned()))
        );
        let mut negative = file.clone();
        negative.objects[waypoint].waypoint.as_mut().unwrap().id = -1;
        assert_eq!(import(&negative), Err(MapError::WaypointId(-1)));
        // A waypoint reads no angle and no height, so neither an infinite angle nor an infinite
        // height refuses it; its ground point past the world's bound does.
        let mut turned = file.clone();
        turned.objects[waypoint].angle = f32::INFINITY;
        turned.objects[waypoint].pos[2] = f32::INFINITY;
        assert_eq!(import(&turned), Ok(()));
        turned.objects[waypoint].pos[1] = f32::INFINITY;
        assert_eq!(import(&turned), Err(MapError::Placement(waypoint)));
        file.objects[1].pos[0] = f32::INFINITY;
        assert_eq!(import(&file), Err(MapError::Placement(1)));
        file.objects[1].pos[0] = 0.0;
        file.objects[1].angle = f32::INFINITY;
        assert_eq!(import(&file), Err(MapError::Placement(1)));
    }
}
