use std::collections::BTreeSet;

use bevy_math::Vec3;
use campfire_client_files::{
    ClientObject, ClientObjects, ClientUnits, HeightGrid, MapConditions, MapLight, MapLights,
    TerrainFile, TerrainSurface,
};
use campfire_codec::{Binary, Toml};
use campfire_geometry::{Bounds, Metric};
use campfire_math::Num;
use campfire_mode::{MapData, MapNavigationData, MapPoint, MarkerData, ModeParam, PlacedUnitData};
use campfire_values::DeclaredName;

use crate::zero_hour::cliff_walls::{CellFrame, CliffWalls};
use crate::zero_hour::error::MapError;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::{ObjectIni, PlacedAs};
use crate::zero_hour::format::look_conditions::LookConditions;
use crate::zero_hour::format::map::map_file::{GameLight, MapFile, MapHeights};
use crate::zero_hour::format::map::map_object::Placement;
use crate::zero_hour::format::map::terrain::Terrain;
use crate::zero_hour::game_data::GameData;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::terrain_atlas::{ClassTexture, TerrainAtlas};
use crate::zero_hour::terrain_cells::TerrainCells;

/// One map's files in the package: `map/<name>/map.toml`, `map/<name>/heights.bin`,
/// `client/maps/<name>/terrain.bin`, `client/maps/<name>/lights.toml` and
/// `client/maps/<name>/objects.toml` and `client/maps/<name>/conditions.toml`; how many cells its
/// terrain draws a tile of with no texture; and the names of the waypoints it dropped, past its
/// bounds.
#[derive(Debug)]
pub(crate) struct MapImport {
    pub(crate) map: String,
    pub(crate) heights: Vec<u8>,
    pub(crate) terrain: Vec<u8>,
    pub(crate) untextured: usize,
    pub(crate) lights: String,
    pub(crate) objects: String,
    pub(crate) conditions: String,
    pub(crate) dropped_waypoints: Vec<String>,
}

/// A waypoint of a map's objects: its id, its name and its ground point.
#[derive(Debug)]
struct Waypoint {
    id: u32,
    name: String,
    pos: [Num; 2],
}

/// A waypoint a skirmish starts a slot's player at, by the slot's number from 1: its start, or
/// its rally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkirmishPoint {
    Start(u8),
    Rally(u8),
}

impl Waypoint {
    /// The skirmish's point it is, as `placeNetworkBuildingsForPlayer` finds them by name:
    /// `Player_<n>_Start` or `Player_<n>_Rally`, `n` from 1 to `SLOTS`, of that case.
    fn skirmish(&self) -> Option<SkirmishPoint> {
        let rest = self.name.strip_prefix("Player_")?;
        let (slot, point): (&str, fn(u8) -> SkirmishPoint) = match rest.strip_suffix("_Start") {
            Some(slot) => (slot, SkirmishPoint::Start),
            None => (rest.strip_suffix("_Rally")?, SkirmishPoint::Rally),
        };
        (1..=SLOTS).find(|n| n.to_string() == slot).map(point)
    }
}

impl SkirmishPoint {
    /// Whether it is a point of one of the slots `1..=slots`.
    const fn of_slots(self, slots: u8) -> bool {
        let (Self::Start(n) | Self::Rally(n)) = self;
        n <= slots
    }
}

/// What a map import reads and adds to beside its file: the object INI and the locomotors its
/// objects name, the names the objects took so far, with the unit types of the maps, the looks of
/// the package's `client/units.toml`, how many of the map's lights the game draws, and the
/// settings of `GameData.ini` its terrain is drawn by.
#[derive(Debug)]
pub(crate) struct MapSources<'a> {
    pub(crate) ini: &'a ObjectIni,
    pub(crate) locomotors: &'a LocomotorIni,
    pub(crate) names: &'a mut ImportNames,
    pub(crate) looks: &'a ClientUnits,
    pub(crate) lights: usize,
    pub(crate) game: GameData,
    /// The texture of each of the map's texture classes, in their order.
    pub(crate) textures: &'a [Option<ClassTexture>],
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

/// A cell of the pathfinder, 10 units wide (`PATHFIND_CELL_SIZE`), which the navigation's cells
/// take (design 17, D3).
const PATHFIND_CELL: i64 = 10;

/// A step of height, 0.625 units (`MAP_HEIGHT_SCALE`): 5 · 2⁻³, exact in a `Num`.
const STEP: Num = Num::from_bits(5 << (Num::FRAC_BITS - 3));

/// The team every placed unit stands on until the import reads the map's sides.
const NEUTRAL: &str = "neutral";

/// The tag of a marker made from a waypoint.
const WAYPOINT: &str = "waypoint";

/// The param of a marker made from a waypoint that holds the waypoint's name.
const WAYPOINT_NAME: &str = "name";

/// The tag beside `waypoint` of a skirmish's start and rally waypoints, which the mode spawns its
/// players' start objects at (design 17, D6).
pub(crate) const START: &str = "start";

/// The player slots of a game, `MAX_SLOTS`: the starts a map may name, from `Player_1_Start`,
/// and the players of a skirmish, each its own team, as the original's default skirmish holds
/// them (design 19, D1).
pub(crate) const SLOTS: u8 = 8;

impl MapImport {
    /// The files of `file`, of `sources`: its heights on the engine's axes, its terrain over them
    /// as a terrain file, its texture the atlas the game builds of its classes; its objects as a
    /// multiplayer game spawns them (design 12, D14), none of a template no INI object names, an
    /// optimized tree, a prop and fluff as client objects, of a look of `looks`, or none when it
    /// has none, and every other as a unit of the type `names` gives it, the bridge-like ones
    /// first, in the map's order; bounds that hold its heights and each unit's body whole; its
    /// waypoints within them as markers, and the names of those past them, which it drops; the
    /// walls of its cliffs within them (design 17, D3); and its first `lights` object lights, as
    /// the game draws `NumberGlobalLights` of each, which the terrain is lit by too; their ambient
    /// color is the first light's, as the game takes it.
    pub(crate) fn new(file: &MapFile, sources: MapSources<'_>) -> Result<Self, MapError> {
        let MapSources {
            ini,
            locomotors,
            names,
            looks,
            lights,
            game,
            textures,
        } = sources;
        let heights = Self::heights(&file.heights)?;
        let map_lights = MapLights::new(
            file.lights[0].ambient,
            file.lights[..lights].iter().map(|light| MapLight {
                color: light.diffuse,
                direction: Self::axes(light),
            }),
        )
        .ok_or(MapError::LightDirection)?;
        let terrain =
            Terrain::new(file.terrain.clone(), file.heights.width).map_err(MapError::Terrain)?;
        let atlas = TerrainAtlas::new(terrain.parts(), textures);
        let cells = TerrainCells::new(terrain.parts(), &file.heights, &atlas, game);
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
        let declared =
            |name: &str| DeclaredName::new(name).expect("the import's own names are declared");
        let (neutral, waypoint_tag, waypoint_name, start_tag) = (
            declared(NEUTRAL),
            declared(WAYPOINT),
            declared(WAYPOINT_NAME),
            declared(START),
        );
        let mut bridges = Vec::new();
        let mut units = Vec::new();
        let mut client = ClientObjects::default();
        let mut waypoints = Vec::new();
        let mut ids = BTreeSet::new();
        for (at, object) in file.objects.iter().enumerate() {
            if object.road_or_bridge() {
                continue;
            }
            if let Some(waypoint) = &object.waypoint {
                let pos = object.ground().ok_or(MapError::Placement(at))?;
                let id = u32::try_from(waypoint.id)
                    .ok()
                    .ok_or(MapError::WaypointId(waypoint.id))?;
                if !ids.insert(id) {
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
                waypoints.push(Waypoint { id, name, pos });
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
                    let look = names.take(template)?.declared();
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
                let placed_type =
                    names
                        .unit_type(template, locomotors)
                        .map_err(|error| MapError::UnitType {
                            object: template.name.clone(),
                            error,
                        })?;
                // The bounds hold its whole body, as a placed box's load needs.
                if let Some(reach) = placed_type.imported.reach(degrees) {
                    for corner in [[-1, -1], [1, 1]] {
                        take([0, 1].map(|axis| ground[axis] + reach[axis] * corner[axis]));
                    }
                }
                let unit = PlacedUnitData {
                    unit_type: placed_type.name,
                    team: neutral.clone(),
                    pos: MapPoint::Ground(take(ground)),
                    angle: degrees,
                    height,
                    path: None,
                    from: None,
                };
                match placed {
                    PlacedAs::Bridge => bridges.push(unit),
                    _ => units.push(unit),
                }
            }
        }
        bridges.append(&mut units);
        let bounds = Bounds::new(low, high).ok_or(MapError::Bounds)?;
        // A waypoint past the heights and the placed bodies marks no place of the map, as a
        // camera's point far off does: it is dropped, and reported.
        // The original lists a map as a skirmish's of as many players as it has starts from
        // `Player_1_Start` on in a row, two or more (`WaypointMap::update`), and starts each
        // player at its slot's: those are its markers of `start`.
        let starts = (1..=SLOTS).take_while(|&n| {
            let start = Some(SkirmishPoint::Start(n));
            waypoints
                .iter()
                .any(|waypoint| waypoint.skirmish() == start)
        });
        let slots = starts.last().filter(|&slots| slots >= 2).unwrap_or(0);
        let mut markers = Vec::new();
        let mut dropped_waypoints = Vec::new();
        for waypoint in waypoints {
            let mut tags = vec![waypoint_tag.clone()];
            if waypoint
                .skirmish()
                .is_some_and(|point| point.of_slots(slots))
            {
                tags.push(start_tag.clone());
            }
            let Waypoint { id, name, pos } = waypoint;
            if bounds.clamp_ground(pos) != pos {
                dropped_waypoints.push(name);
                continue;
            }
            markers.push(MarkerData {
                name: declared(&format!("w{id}")),
                tags,
                pos: Some(MapPoint::Ground(pos)),
                region: None,
                team: None,
                params: [(waypoint_name.clone(), ModeParam::Text(name))].into(),
            });
        }
        let cliffs: Vec<bool> = file.terrain.cells.iter().map(|cell| cell.cliff).collect();
        let frame = CellFrame {
            north_west: heights.origin(),
            cell: Num::int(CELL),
            rows: file.heights.depth,
        };
        let walls = CliffWalls::of(&cliffs, file.heights.width).walls(frame, bounds);
        let map = MapData {
            metric: Metric::Planar,
            bounds,
            grid: None,
            navigation: Some(MapNavigationData {
                cell: Num::int(PATHFIND_CELL),
                walls,
            }),
            paths: Vec::new(),
            units: bridges,
            markers,
        };
        Ok(Self {
            map: Toml::write(&map),
            heights: Binary::encode(&heights),
            terrain: Binary::encode(&terrain),
            untextured,
            lights: Toml::write(&map_lights),
            objects: Toml::write(&client),
            conditions: Toml::write(&MapConditions {
                conditions: condition_names(file.conditions),
            }),
            dropped_waypoints,
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
            Self::flipped(samples, *width),
        )
        .ok_or(MapError::Heights)
    }

    /// `rows`, each `width` entries, in reverse.
    fn flipped<T: Copy>(rows: &[T], width: usize) -> Vec<T> {
        rows.rchunks_exact(width).flatten().copied().collect()
    }
}

/// The names the package gives `conditions`, `night` and `snow`, in that order.
pub(crate) fn condition_names(conditions: LookConditions) -> Vec<DeclaredName> {
    let named = |name| DeclaredName::new(name).expect("the import's own names are declared");
    [(conditions.night, "night"), (conditions.snow, "snow")]
        .into_iter()
        .filter(|&(held, _)| held)
        .map(|(_, name)| named(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::mem;

    use campfire_client_files::ClientUnit;

    use super::*;
    use crate::zero_hour::format::ini::locomotor_ini::internals::LOCOMOTORS;
    use crate::zero_hour::format::ini::object_ini::internals::{
        DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS,
    };
    use crate::zero_hour::format::map::map_file::internals::map;
    use crate::zero_hour::format::map::terrain::{BlendShape, BlendTile};

    /// The fixture map's `data/units.toml`.
    const UNITS_TOML: &str = r#"[units.americatankcrusader]
pools = ["life"]

[units.americatankcrusader.stats.move_speed]
base = "30.5"

[units.americatankcrusader.stats.turn_rate]
base = "90"

[units.americatankcrusader.stats.life]
base = "400"

[units.americatankcrusader.collision]
radius = "5"

[units.americatankcrusader.combat]
on_death = "despawn"

[units.fortresswall.collision]
box = ["20", "5"]

[units.rocks1]
"#;

    /// The fixture install's objects.
    fn objects() -> ObjectIni {
        let mut ini = ObjectIni::default();
        for objects in [DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS] {
            ini.read(objects.as_bytes()).unwrap();
        }
        ini
    }

    /// The import of `file` of the fixture install's objects, with the looks of its client
    /// objects but the prop of no model's, drawn in `lights` lights.
    fn import(
        file: &MapFile,
        names: &mut ImportNames,
        lights: usize,
    ) -> Result<MapImport, MapError> {
        let ini = objects();
        let objects = ["oak", "rock", "rockred", "bush"].map(|look| {
            (
                DeclaredName::new(look).unwrap(),
                ClientUnit::new(Vec::new()),
            )
        });
        let looks = ClientUnits {
            objects: objects.into(),
            ..ClientUnits::default()
        };
        let mut locomotors = LocomotorIni::default();
        locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
        let sources = MapSources {
            ini: &ini,
            locomotors: &locomotors,
            names,
            looks: &looks,
            lights,
            game: GameData {
                adjust_cliff_textures: true,
                three_way_blends: true,
            },
            textures: &vec![None; file.terrain.classes.len()],
        };
        MapImport::new(file, sources)
    }

    #[test]
    fn a_map_s_lights_are_its_time_of_day_s_on_the_engine_s_axes() {
        let mut file = MapFile::read(&map(8)).unwrap();
        let mut names = ImportNames::default();
        let imported = import(&file, &mut names, 3).unwrap();
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
        let two = import(&file, &mut names, 2).unwrap();
        assert_eq!(
            Toml::parse::<MapLights>(&two.lights).unwrap().lights.len(),
            2
        );
        // An object light it draws that goes no way is refused; one it does not draw is not.
        file.lights[1].position = [0.0; 3];
        let refused = import(&file, &mut names, 3).map(drop);
        assert_eq!(refused, Err(MapError::LightDirection));
        assert!(import(&file, &mut names, 1).is_ok());
    }

    #[test]
    fn a_map_imports_to_its_heights_units_and_markers_computed_by_hand() {
        let mut file = MapFile::read(&map(8)).unwrap();
        let mut names = ImportNames::default();
        let imported = import(&file, &mut names, 3).unwrap();

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
                    ModeParam::Text("Fixture_Center".to_owned()),
                )]
                .into(),
            }]
        );
        // The bounds hold the heights, the rock's z of −15, and the tank's body, of radius 5 at
        // x 30, to x 35; the wall's box, of reach 10 and 2.5 from (10, 0), lies within; not the
        // tree's x of 50, which the sim does not hold.
        let bounds = Toml::parse("min = [-10, -15]\nmax = [35, 10]").unwrap();
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
        // A day map of fair weather sets no condition: its looks draw their own models. A night
        // map sets `night`, a snowy one `snow`, and a snowy night both.
        assert_eq!(imported.conditions, "conditions = []\n");
        let mut dark = file.clone();
        for (night, snow, conditions) in [
            (true, false, r#"["night"]"#),
            (false, true, r#"["snow"]"#),
            (true, true, r#"["night", "snow"]"#),
        ] {
            dark.conditions = LookConditions { night, snow };
            let written = import(&dark, &mut ImportNames::default(), 3).unwrap();
            assert_eq!(written.conditions, format!("conditions = {conditions}\n"));
        }
        // Its navigation's cells are the pathfinder's, of 10 m. Its cliffs, the game's cells
        // [0, 0] and [3, 2] of 4 × 3, grow over [0, 0] to [1, 1] and [2, 1] to [3, 2]: the runs 0
        // to 1 of row 0, 0 to 3 of row 1 and 2 to 3 of row 2. The game's row `y` of its 3 rows of
        // samples spans z from −10 + 20 − 10 (y + 1) = −10 y to 10 − 10 y, as the heights' north
        // west is (−10, −10), and column `x` x from −10 + 10 x. In the bounds, the walls are x
        // −10 to 10 by z 0 to 10, x −10 to 30 by z −10 to 0, and x 10 to 30 by z −20 to −10 cut
        // at the bounds' z −15.
        let navigation = map.navigation.unwrap();
        assert_eq!(navigation.cell, Num::int(10));
        let square = |[x0, z0, x1, z1]: [i64; 4]| {
            [[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
                .map(|[x, z]| MapPoint::Ground([Num::int(x), Num::int(z)]))
                .to_vec()
        };
        let walls: Vec<_> = navigation
            .walls
            .into_iter()
            .map(|wall| wall.points)
            .collect();
        assert_eq!(
            walls,
            [
                square([-10, 0, 10, 10]),
                square([-10, -10, 30, 0]),
                square([10, -15, 30, -10]),
            ]
        );
        // The tank drives on its treads, 30.5 m and 90° a second, a circle of √(4² + 3²) = 5 m,
        // and lives by its active body, 400, of no armor; the wall is a structure, a box of 2 ·
        // 10 by 2 · 2.5 m, whose highlander body gives it no life; the fence is neither.
        let units = mem::take(&mut names).close().bare_file();
        assert_eq!(units.text, UNITS_TOML);
        assert_eq!(units.max_speed, Some(Num::int(61) / Num::int(2)));

        // A waypoint takes no place in the bounds. Moved to the bounds' x of 35 it stays; past
        // it, at 35.5, it is dropped by its name, and the bounds stay as they are.
        assert!(imported.dropped_waypoints.is_empty());
        let waypoint = 2;
        for (x, kept) in [(35.0, true), (35.5, false)] {
            file.objects[waypoint].pos[0] = x;
            let moved = import(&file, &mut names, 3).unwrap();
            let map = Toml::parse::<MapData>(&moved.map).unwrap();
            assert_eq!(map.bounds, bounds);
            assert_eq!(map.markers.len(), usize::from(kept));
            let dropped: &[&str] = if kept { &[] } else { &["Fixture_Center"] };
            assert_eq!(moved.dropped_waypoints, dropped);
        }

        // A map of starts from `Player_1_Start` on in a row, two or more, is a skirmish's: each
        // start and rally of those slots is tagged `start` too, the count of tags 2. One past a
        // gap in the row, a map of one start or none of slot 1, and another slot's, case's or
        // name's waypoint are not: 1.
        file.objects[waypoint].pos[0] = 12.5;
        let base = file.objects[waypoint].clone();
        let tags = |names: &[&str]| {
            let mut placed = file.clone();
            placed.objects.retain(|object| object.waypoint.is_none());
            for (id, name) in (10..).zip(names) {
                let mut object = base.clone();
                let point = object.waypoint.as_mut().unwrap();
                (point.id, point.name) = (id, name.as_bytes().to_vec());
                placed.objects.push(object);
            }
            let markers = Toml::parse::<MapData>(
                &import(&placed, &mut ImportNames::default(), 3).unwrap().map,
            )
            .unwrap()
            .markers;
            markers
                .iter()
                .map(|marker| marker.tags.len())
                .collect::<Vec<_>>()
        };
        let row = [
            "Player_1_Start",
            "Player_2_Start",
            "Player_2_Rally",
            "Player_4_Start",
        ];
        assert_eq!(tags(&row), [2, 2, 2, 1]);
        assert_eq!(tags(&["Player_1_Start", "Player_1_Rally"]), [1, 1]);
        assert_eq!(tags(&["Player_2_Start", "Player_3_Start"]), [1, 1]);
        let eight: Vec<String> = (1..=8).map(|n| format!("Player_{n}_Start")).collect();
        let mut all: Vec<&str> = eight.iter().map(String::as_str).collect();
        all.push("Player_8_Rally");
        assert_eq!(tags(&all), [2; 9]);
        let others = [
            "Player_1_Start",
            "Player_2_Start",
            "Player_02_Rally",
            "player_1_rally",
            "Player_1_Spawn",
            "Player_9_Start",
            "Player_0_Start",
        ];
        assert_eq!(tags(&others), [2, 2, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn a_map_whose_waypoints_or_objects_the_engine_cannot_hold_is_refused() {
        let mut file = MapFile::read(&map(8)).unwrap();
        let import = |file: &MapFile| import(file, &mut ImportNames::default(), 3).map(drop);
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
