use campfire_codec::{Toml, TomlError, TomlTable};
use campfire_navigation::{MapNavigationData, PathWalk, WallData};
use campfire_units::{
    CellSplit, CoarseSpan, Ground, GroundError, GroundLoadError, HeightGrid, MapBook, MapSpace,
};

use super::*;

/// The readers of every section a map of a test holds.
fn readers() -> SectionReaders {
    let mut readers = SectionReaders::default();
    readers.add::<MetricData>(None);
    readers.add::<BoundsData>(None);
    readers.add::<GroundData>(None);
    readers.add::<MapNavigationData>(None);
    readers.add::<MapPathsData>(None);
    readers
}

fn read(text: &str) -> Result<MapData, TomlError> {
    Toml::parse::<TomlTable>(text)
        .unwrap()
        .read_with(MapData::seed(&readers()))
}

#[test]
fn a_map_reads_its_own_fields_and_each_section_apart() {
    // Every section: a spatial metric, navigation with a wall on a
    // layer and one on the first, a path, a unit on it, turned and raised, walking it from
    // its end, a unit of no path, and a marker of a region, a team and two params.
    let text = r#"
        metric = "spatial"
        bounds = { min = [-10, -10], max = [10, 10] }
        navigation = { cell = "0.5", walls = [{ layer = "air", points = [[1, 0, 1], [3, 0, 1], [1, 0, 3]] }, { points = [[4, 0, 4], [5, 0, 4], [4, 0, 5]] }] }
        paths = [
            { name = "lane", points = [[0, 0, 0], [5, 0, 5]] },
            { name = "beat", points = [[1, 0, 1]], walk = "back_and_forth" },
        ]
        units = [
            { unit_type = "creep", team = "red", position = [1, 2, 3], angle = "22.5", height = 4, path = "lane", from = "end" },
            { unit_type = "tower", team = "blue", position = [-1, 0, -1] },
        ]
        markers = [{ name = "camp", tags = ["spawn"], region = { min = [0, 0, 0], max = [2, 2, 2] }, team = "red", params = { count = 3, kind = "wolf" } }]
    "#;
    let map = read(text).unwrap();
    let int = Num::int;
    let space = |x, y, z| MapPoint::Space([int(x), int(y), int(z)]);
    assert_eq!(map.metric(), Metric::Spatial);
    assert_eq!(
        map.bounds(),
        Bounds::new([int(-10), int(-10)], [int(10), int(10)]).unwrap()
    );
    assert_eq!(map.ground_data(), None);
    let navigation = map.section::<MapNavigationData>().unwrap();
    assert_eq!(navigation.cell, Num::ONE / int(2));
    assert_eq!(
        navigation.walls[1],
        WallData {
            layer: None,
            points: vec![space(4, 0, 4), space(5, 0, 4), space(4, 0, 5)],
        }
    );
    assert_eq!(
        map.paths(),
        [
            PathData {
                name: DeclaredName::new("lane").unwrap(),
                points: vec![space(0, 0, 0), space(5, 0, 5)],
                walk: PathWalk::Once,
            },
            PathData {
                name: DeclaredName::new("beat").unwrap(),
                points: vec![space(1, 0, 1)],
                walk: PathWalk::BackAndForth,
            }
        ]
    );
    assert_eq!((map.units.len(), map.markers.len()), (2, 1));
    // The map's own fields and its sections are all it reads.
    let stray = read("bounds = { min = [0, 0], max = [1, 1] }\nroads = []").unwrap_err();
    assert!(
        stray.message().starts_with("unknown field `roads`"),
        "{stray}"
    );
    // Every map gives its bounds.
    assert_eq!(
        read("units = []").unwrap_err().message(),
        "missing field `bounds`"
    );
    // A unit at no angle and height, and a marker of no region, team or param, write their
    // other fields alone.
    let unit = PlacedUnitData::new("creep", "red", MapPoint::ground(1, 2));
    assert_eq!(
        Toml::write(&unit),
        "unit_type = \"creep\"\nteam = \"red\"\nposition = [\"1\", \"2\"]\n"
    );
    let marker = MarkerData::tagged("w1", &["waypoint"], MapPoint::ground(0, 0));
    assert_eq!(
        Toml::write(&marker),
        "name = \"w1\"\ntags = [\"waypoint\"]\nposition = [\"0\", \"0\"]\n"
    );
}

#[test]
fn a_placed_unit_stands_its_height_above_the_ground_or_on_it() {
    let map = read(
        r#"
        bounds = { min = [0, 0], max = [4, 4] }
        [[units]]
        unit_type = "rock"
        team = "neutral"
        position = [1, 2]
        angle = "22.5"
        height = "-4.5"
        [[units]]
        unit_type = "rock"
        team = "neutral"
        position = [3, 2]
        "#,
    )
    .unwrap();
    let rock = |position, angle, height| PlacedUnitData {
        angle,
        height,
        ..PlacedUnitData::new("rock", "neutral", position)
    };
    assert_eq!(
        map.units,
        [
            rock(
                MapPoint::ground(1, 2),
                Num::int(45) / Num::int(2),
                Num::int(-9) / Num::int(2)
            ),
            rock(MapPoint::ground(3, 2), Num::ZERO, Num::ZERO),
        ]
    );
}

#[test]
fn a_maps_ground_is_its_height_grid_as_its_ground_section_reads_it() {
    // A map of 3 × 3 samples 10 m apart, from the origin. With no grid it is flat, and a
    // `[ground]` with no grid is refused; with one, its split and coarse cells are those of its
    // `[ground]`, the grid's own cells with none, and coarse cells smaller than its cells are
    // refused.
    let grid = HeightGrid::new(
        [Num::ZERO; 2],
        Num::int(10),
        Num::ONE,
        3,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8],
    )
    .unwrap();
    let span = |cell: i64| CoarseSpan {
        cell: Num::int(cell),
        from: [Num::ZERO; 2],
        to: [Num::int(20); 2],
    };
    let resolve = |ground: Option<GroundData>, heights: Option<&HeightGrid>| {
        let map = MapData::planar(Bounds::new([Num::int(-50); 2], [Num::int(50); 2]).unwrap());
        let map = match ground {
            Some(ground) => map.with(ground),
            None => map,
        };
        MapBook::of(map.sections(), heights).map(|book| match book.space {
            MapSpace::Planar(ground) => ground,
            MapSpace::Spatial => unreachable!("the test's maps are planar"),
        })
    };
    let read = |split, cell| GroundData {
        split,
        high: Some(span(cell)),
    };
    assert_eq!(resolve(None, None), Ok(Ground::default()));
    assert_eq!(
        resolve(Some(GroundData::default()), None),
        Err(GroundLoadError::NoHeights)
    );
    let own = Ground::new(grid.clone(), CellSplit::ZeroTwo, None).unwrap();
    assert_eq!(resolve(None, Some(&grid)), Ok(own));
    let coarse = Ground::new(grid.clone(), CellSplit::OneThree, Some(span(20))).unwrap();
    assert_eq!(
        resolve(Some(read(CellSplit::OneThree, 20)), Some(&grid)),
        Ok(coarse)
    );
    assert_eq!(
        resolve(Some(read(CellSplit::OneThree, 5)), Some(&grid)),
        Err(GroundLoadError::Ground(GroundError::CoarseCell))
    );
}
