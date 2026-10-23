use std::mem;

use campfire_client_files::ObjectLook;
use campfire_codec::TomlTable;
use campfire_declarations::SectionReaders;
use campfire_units::{GroundError, MetricData};

use super::*;
use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::object_ini::samples::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};
use crate::zero_hour::format::map::map_file::samples::map;
use crate::zero_hour::format::map::terrain::{BlendShape, BlendTile};

/// The map of `text`, read as the engine reads the sections the import writes.
fn read_map(text: &str) -> MapData {
    let mut readers = SectionReaders::default();
    readers.add::<MetricData>(None);
    readers.add::<BoundsData>(None);
    readers.add::<GroundData>(None);
    readers.add::<MapNavigationData>(None);
    let raw = Toml::parse::<TomlTable>(text).unwrap();
    raw.read_with(MapData::seed(&readers)).unwrap()
}

/// The fixture map's `data/units.toml`.
const UNITS_TOML: &str = r#"[units.americatankcrusader]
pools = ["life"]

[units.americatankcrusader.collision]
height = "1"
radius = "5"

[units.americatankcrusader.combat]

[units.americatankcrusader.stats.life]
base = "400"

[units.americatankcrusader.stats.move_speed]
base = "30.5"

[units.americatankcrusader.stats.turn_rate]
base = "90"

[units.americatankcrusader.turret]
turn_rate = "180"

[units.fortresswall]
tags = ["structure"]

[units.fortresswall.collision]
box = ["20", "5"]
height = "1"

[units.rocks1]
tags = ["wall"]
"#;

/// The fixture install's objects.
fn objects() -> ObjectIni {
    let mut ini = ObjectIni::default();
    for objects in [DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS] {
        ini.read(objects.as_bytes()).unwrap();
    }
    ini
}

/// The fixture install's objects and locomotors, and the looks of its client objects but the
/// prop of no model's, which a map's import reads.
#[derive(Debug)]
struct MapFixture {
    rules: ZeroHourRules,
    looks: ClientUnits,
}

impl MapFixture {
    fn new() -> Self {
        let mut rules = ZeroHourRules {
            objects: objects(),
            ..ZeroHourRules::default()
        };
        rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
        let objects = ["oak", "rock", "rockred", "bush"].map(|look| {
            (
                DeclaredName::new(look).unwrap(),
                ObjectLook::new(Vec::new()),
            )
        });
        let looks = ClientUnits {
            objects: objects.into(),
            ..ClientUnits::default()
        };
        Self { rules, looks }
    }

    /// The import of `file`, its objects named in `names`, drawn in `lights` lights.
    fn import<'r>(
        &'r self,
        file: &ZeroHourMap,
        names: &mut ImportNames<'r>,
        lights: usize,
    ) -> Result<MapImport, MapError> {
        let sources = MapSources {
            rules: &self.rules,
            names,
            looks: &self.looks,
            lights,
            game: TerrainDraw {
                adjust_cliff_textures: true,
                three_way_blends: true,
            },
            textures: &vec![None; file.terrain.fields().classes.len()],
        };
        MapImport::new(file, sources)
    }
}

#[test]
fn a_map_s_lights_are_its_time_of_day_s_on_the_engine_s_axes() {
    let fixture = MapFixture::new();
    let mut file = ZeroHourMap::read(&map(8)).unwrap();
    let mut names = ImportNames::default();
    let imported = fixture.import(&file, &mut names, 3).unwrap();
    // The afternoon's lights: light `l` of time 2 has ambient [2, l, 0], diffuse [2, l, 1]
    // and position [2, l, 2], which is the engine's [2, 2, −l]. The objects' lights are 3 to
    // 5, after the terrain's 0 to 2, which the import drops; their ambient is light 3's.
    let object_light = |at: f32| MapLight {
        color: [2.0, at, 1.0],
        direction: Vec3::new(2.0, 2.0, -at),
    };
    assert_eq!(
        MapLights::decode(&imported.lights).unwrap(),
        MapLights {
            ambient: [2.0, 3.0, 0.0],
            lights: vec![object_light(3.0), object_light(4.0), object_light(5.0)],
        }
    );
    // Drawn in two lights, each keeps its first two.
    let two = fixture.import(&file, &mut names, 2).unwrap();
    assert_eq!(MapLights::decode(&two.lights).unwrap().lights.len(), 2);
    // An object light it draws that goes no way is refused; one it does not draw is not.
    file.lights[1].position = [0.0; 3];
    let refused = fixture.import(&file, &mut names, 3).map(drop);
    assert_eq!(refused, Err(MapError::LightDirection));
    assert!(fixture.import(&file, &mut names, 1).is_ok());
}

#[test]
fn a_placed_variation_stands_as_its_parents_type_of_its_place_and_its_own_body() {
    // The tank as the second variation of a jeep, whose first is the tank of a radius of 30: the
    // map's tank stands as the jeep of the second variation, and the bounds hold its own body, of
    // radius 5 at x 30, as the import of the tank alone does
    // (09-zero-hour.md, A chassis converts to the parent of its file).
    let mut fixture = MapFixture::new();
    let jeep = "Object Jeep\n BuildVariations = Wide AmericaTankCrusader\nEnd\n\
                ObjectReskin Wide AmericaTankCrusader\n GeometryMajorRadius = 30.0\nEnd\n";
    fixture.rules.objects.read(jeep.as_bytes()).unwrap();
    let file = ZeroHourMap::read(&map(8)).unwrap();
    let mut names = ImportNames::default();
    let map = read_map(&fixture.import(&file, &mut names, 3).unwrap().map);
    let jeep = &map.units[1];
    assert_eq!((jeep.unit_type.as_str(), jeep.variation), ("jeep", Some(1)));
    let bounds = Toml::parse("min = [-10, -15]\nmax = [35, 10]").unwrap();
    assert_eq!(map.bounds(), bounds);
}

#[test]
fn a_map_imports_to_its_heights_units_and_markers_computed_by_hand() {
    let fixture = MapFixture::new();
    let mut file = ZeroHourMap::read(&map(8)).unwrap();
    let mut names = ImportNames::default();
    let imported = fixture.import(&file, &mut names, 3).unwrap();

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
    let cells = MapImport::flipped(&file.terrain.fields().cells, file.heights.width);
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
        file.terrain.fields().blends,
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
    let map = read_map(&imported.map);
    let decimal = |text: &str| text.parse::<Num>().unwrap();
    let ground = |x: &str, z: &str| MapPoint::Ground([decimal(x), decimal(z)]);
    let unit = |unit_type: &str, position, angle: i64, height: Num| PlacedUnitData {
        unit_type: DeclaredName::new(unit_type).unwrap(),
        team: DeclaredName::new("neutral").unwrap(),
        position,
        angle: Num::from_bits(angle),
        height,
        path: None,
        from: None,
        variation: None,
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
            position: Some(ground("12.5", "-7.5")),
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
    assert_eq!(map.bounds(), bounds);
    // The ground of the original's logic: split on the file's 1 to 3, and the partition's
    // cells of 40 from the world's corner over the boundary of 2 × 1 cells, x 0 to 20 and the
    // original's y 0 to 10, the engine's z 0 to −10. A boundary past the heights, 9 cells to
    // x 90, holds 3 × 1 cells, fewer than its 12 samples, and loads; one of 100 × 100 cells,
    // 25 × 25 partition cells, more than its samples, is refused.
    let span = CoarseSpan {
        cell: Num::int(40),
        from: [Num::ZERO; 2],
        to: [Num::int(20), Num::int(-10)],
    };
    let ground = GroundData {
        split: CellSplit::OneThree,
        high: Some(span),
    };
    assert_eq!(map.ground_data(), Some(ground));
    let mut wide = file.clone();
    wide.heights.boundary = Some([9, 1]);
    assert!(fixture.import(&wide, &mut names, 3).is_ok());
    wide.heights.boundary = Some([100, 100]);
    let refused = fixture.import(&wide, &mut names, 3).map(drop);
    assert_eq!(refused, Err(MapError::Ground(GroundError::CoarseCount)));
    // The tree, the prop 1 above the ground, its reskin and the fluff are client objects, in
    // the map's order, of their templates' looks; the prop of no look, the template no object
    // names and the one of another case are none; the fence that a build clears is a unit.
    let client = |look: &str, x: i64, z: i64, height: i64| ClientObject {
        look: DeclaredName::new(look).unwrap(),
        position: [Num::int(x), Num::int(z)],
        height: Num::int(height),
        angle: Num::ZERO,
    };
    assert_eq!(
        ClientObjects::decode(&imported.objects).unwrap(),
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
    assert_eq!(imported.conditions, b"conditions = []\n");
    let mut dark = file.clone();
    for (night, snow, conditions) in [
        (true, false, r#"["night"]"#),
        (false, true, r#"["snow"]"#),
        (true, true, r#"["night", "snow"]"#),
    ] {
        dark.conditions = LookConditions { night, snow };
        let written = fixture
            .import(&dark, &mut ImportNames::default(), 3)
            .unwrap();
        assert_eq!(
            written.conditions,
            format!("conditions = {conditions}\n").into_bytes()
        );
    }
    // Its navigation's cells are the pathfinder's, of 10 m. Its cliffs, the game's cells
    // [0, 0] and [3, 2] of 4 × 3, grow over [0, 0] to [1, 1] and [2, 1] to [3, 2]: the runs 0
    // to 1 of row 0, 0 to 3 of row 1 and 2 to 3 of row 2. The game's row `y` of its 3 rows of
    // samples spans z from −10 + 20 − 10 (y + 1) = −10 y to 10 − 10 y, as the heights' north
    // west is (−10, −10), and column `x` x from −10 + 10 x. In the bounds, the walls are x
    // −10 to 10 by z 0 to 10, x −10 to 30 by z −10 to 0, and x 10 to 30 by z −20 to −10 cut
    // at the bounds' z −15.
    let navigation = map.section::<MapNavigationData>().unwrap();
    assert_eq!(navigation.cell, Num::int(10));
    let square = |[x0, z0, x1, z1]: [i64; 4]| {
        [[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
            .map(|[x, z]| MapPoint::Ground([Num::int(x), Num::int(z)]))
            .to_vec()
    };
    let walls: Vec<_> = (navigation.walls.iter().cloned())
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
    // lives by its active body, 400, of no armor, and turns its gun on a turret of 180° a
    // second within 33,644,228 · 2⁻²⁴°; the wall is a structure, a box of 2 · 10 by 2 · 2.5
    // m, whose highlander body gives it no life; the fence is a wall of no body. Each body
    // has the game's default height, 1 m, as no geometry names its own.
    let units = mem::take(&mut names)
        .into_production()
        .into_projectiles()
        .close()
        .bare_file();
    assert_eq!(units.text, UNITS_TOML);
    assert_eq!(units.max_speed, Some(Num::int(61) / Num::int(2)));

    // A waypoint takes no place in the bounds. Moved to the bounds' x of 35 it stays; past
    // it, at 35.5, it is dropped by its name, and the bounds stay as they are.
    assert!(imported.dropped_waypoints.is_empty());
    let waypoint = 2;
    for (x, kept) in [(35.0, true), (35.5, false)] {
        file.objects[waypoint].position[0] = x;
        let moved = fixture.import(&file, &mut names, 3).unwrap();
        let map = read_map(&moved.map);
        assert_eq!(map.bounds(), bounds);
        assert_eq!(map.markers.len(), usize::from(kept));
        let dropped: &[&str] = if kept { &[] } else { &["Fixture_Center"] };
        assert_eq!(moved.dropped_waypoints, dropped);
    }

    // A map of starts from `Player_1_Start` on in a row, two or more, is a skirmish's: each
    // start and rally of those slots is tagged `start` too, the count of tags 2. One past a
    // gap in the row, a map of one start or none of slot 1, and another slot's, case's or
    // name's waypoint are not: 1.
    file.objects[waypoint].position[0] = 12.5;
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
        let markers = read_map(
            &fixture
                .import(&placed, &mut ImportNames::default(), 3)
                .unwrap()
                .map,
        )
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
    let fixture = MapFixture::new();
    let mut file = ZeroHourMap::read(&map(8)).unwrap();
    let import = |file: &ZeroHourMap| {
        fixture
            .import(file, &mut ImportNames::default(), 3)
            .map(drop)
    };
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
    turned.objects[waypoint].position[2] = f32::INFINITY;
    assert_eq!(import(&turned), Ok(()));
    turned.objects[waypoint].position[1] = f32::INFINITY;
    assert_eq!(import(&turned), Err(MapError::Placement(waypoint)));
    file.objects[1].position[0] = f32::INFINITY;
    assert_eq!(import(&file), Err(MapError::Placement(1)));
    file.objects[1].position[0] = 0.0;
    file.objects[1].angle = f32::INFINITY;
    assert_eq!(import(&file), Err(MapError::Placement(1)));
}
