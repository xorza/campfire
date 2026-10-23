use campfire_geometry::{Bounds, PolygonError};
use campfire_math::Num;
use campfire_navigation::{
    MapNavigationData, MapPathsData, MapProblem, ModeNavigationData, Navigation,
    NavigationDataError, PathData, PathWalk, WalkCheck, Walker, WalkerKinds, WallData,
};
use campfire_units::{BodyShape, Layer, MapBook, MapPointError};
use campfire_values::NameId;

use super::*;
use crate::map_data::{MarkerData, PlacedUnitData};

/// The second layer, as an RTS's air.
const AIR: Layer = Layer::new(1);

/// A walker of `radius` on the first layer.
const fn ground(radius: Num) -> Walker {
    Walker {
        layer: Layer::FIRST,
        radius,
    }
}

/// A corridor 10 m by 4 m in half-meter cells, a lane along z = 2 from (1, 2) to (9, 2), towers
/// at `towers`, a creep on the west spawn at (1, 1), which walks and so blocks nothing, and a camp
/// marker at `camp`.
fn corridor(towers: &[(i64, i64)], camp: (i64, i64)) -> MapData {
    let point = MapPoint::ground;
    let placed = |unit_type, (x, z)| PlacedUnitData::new(unit_type, "west", point(x, z));
    let marker = |name, (x, z)| MarkerData::tagged(name, &[name], point(x, z));
    let bounds = Bounds::new([Num::int(0), Num::int(0)], [Num::int(10), Num::int(4)]).unwrap();
    let mut map = MapData::planar(bounds)
        .with(MapNavigationData {
            cell: Num::HALF,
            walls: Vec::new(),
        })
        .with(MapPathsData(vec![PathData {
            name: DeclaredName::new("lane").unwrap(),
            points: vec![point(1, 2), point(9, 2)],
            walk: PathWalk::Once,
        }]));
    map.units = towers
        .iter()
        .map(|&at| placed("tower", at))
        .chain([placed("creep", (1, 1))])
        .collect();
    map.markers = vec![marker("spawn", (1, 1)), marker("camp", camp)];
    map
}

/// The layers `rules` declares, by layer.
fn layer_table(rules: &ModeNavigationData) -> Names<Layer> {
    rules.layers.iter().map(DeclaredName::as_str).collect()
}

/// `map` resolved as a mode of one team, `west`, of `rules`, and checked walkable for `walkers`,
/// its placed units' bodies by their types' names as `body_of` gives them.
fn check_walkable(
    map: &MapData,
    walkers: &[Walker],
    rules: &ModeNavigationData,
    body_of: impl Fn(&str) -> Option<BodyForm>,
) -> Result<(), MapProblem> {
    const TYPES: [&str; 4] = ["tower", "creep", "cloud", "crate"];
    let unit_type = |name: &str| {
        let at = TYPES.iter().position(|&held| held == name)?;
        Some(UnitType::new(u16::try_from(at).unwrap()))
    };
    let teams = [TeamManifest {
        name: DeclaredName::new("west").unwrap(),
        player_slots: 0,
    }];
    let layers = layer_table(rules);
    let ground = MapBook::of(map.sections(), None).unwrap();
    let navigation = NavigationBook::of(&ground, map.sections()).unwrap();
    let walls = Navigation::walls_of(&ground, map.sections(), Some(&layers)).unwrap();
    let input = MapInput {
        map,
        relations: &[],
        teams: ModeMap::team_names(&teams).unwrap(),
        paths: Some(&navigation.paths),
        variations: None,
    };
    let map = ModeMap::of(input, unit_type).unwrap();
    let name = |unit_type: UnitType| TYPES[unit_type.index()];
    let named = |unit_type| DeclaredName::new(name(unit_type)).unwrap();
    let body = |unit_type, _| body_of(name(unit_type));
    let kinds: WalkerKinds = walkers.iter().copied().collect();
    let check = WalkCheck {
        ground: &ground,
        navigation: &navigation,
        walls: &walls,
        walkers: &kinds,
        placed: &map.placed_bodies(body),
        spawns: &map.spawn_points(&rules.spawn_tags),
    };
    check.run(named)
}

/// The corridor's bodies: a tower of 0.9 m on the ground, a cloud of the same width in the air.
fn corridor_body(unit_type: &str) -> Option<BodyForm> {
    let tower = BodyShape::circle(Num::from_bits((9 << Num::FRAC_BITS) / 10)).unwrap();
    match unit_type {
        "tower" => Some(BodyForm::new(tower, Layer::FIRST)),
        "cloud" => Some(BodyForm::new(tower, AIR)),
        _ => None,
    }
}

#[test]
fn a_map_loads_only_if_the_widest_walker_reaches_every_waypoint_and_stands_on_every_marker() {
    // Walkers of 0.5 m in the corridor. Towers at (5, 0) and (5, 4) block the centers closer than
    // 1.4 m, z up to 1.25 and from 2.75 at x = 4.75 and 5.25, which leaves z = 1.75 and 2.25 open:
    // a gap a walker passes. One more at (5, 2) closes it.
    let half = Num::HALF;
    let point = MapPoint::ground;
    let placed = |unit_type, (x, z)| PlacedUnitData::new(unit_type, "west", point(x, z));
    let rules = ModeNavigationData {
        spawn_tags: ["spawn", "camp"]
            .map(|tag| DeclaredName::new(tag).unwrap())
            .into(),
        ..ModeNavigationData::default()
    };
    let check = |towers: &[(i64, i64)], neutral| {
        check_walkable(
            &corridor(towers, neutral),
            &[ground(half)],
            &rules,
            corridor_body,
        )
    };
    assert_eq!(check(&[(5, 0), (5, 4)], (8, 3)), Ok(()));
    let unreachable = MapProblem::WaypointUnreachable {
        path: DeclaredName::new("lane").unwrap(),
        waypoint: 1,
    };
    assert_eq!(check(&[(5, 0), (5, 2), (5, 4)], (8, 3)), Err(unreachable));
    // A tower 1 m from the lane's end, (9, 2), or from the spawn marker, (1, 1), or from the camp
    // marker, (8, 3): closer than 1.4 m.
    let blocked = MapProblem::WaypointBlocked {
        path: DeclaredName::new("lane").unwrap(),
        waypoint: 1,
    };
    assert_eq!(check(&[(9, 3)], (8, 1)), Err(blocked));
    let blocked = |marker: &str| {
        let marker = DeclaredName::new(marker).unwrap();
        Err(MapProblem::MarkerBlocked { marker })
    };
    assert_eq!(check(&[(2, 1)], (8, 3)), blocked("spawn"));
    assert_eq!(check(&[(7, 3)], (8, 3)), blocked("camp"));
    // A marker of no spawn tag of the mode's is not checked: with `spawn` alone, the tower by the
    // camp loads.
    let spawn_only = ModeNavigationData {
        spawn_tags: vec![DeclaredName::new("spawn").unwrap()],
        ..ModeNavigationData::default()
    };
    let by_camp = corridor(&[(7, 3)], (8, 3));
    assert_eq!(
        check_walkable(&by_camp, &[ground(half)], &spawn_only, corridor_body),
        Ok(())
    );

    // Each layer's widest walker is checked against the bodies of its layer alone: an air walker
    // passes over the towers that close the lane, and a cloud by the camp blocks it there, but
    // not a walker on the ground.
    let flyer = Walker {
        layer: AIR,
        radius: half,
    };
    let both = [ground(half), flyer];
    let closed = corridor(&[(5, 0), (5, 2), (5, 4)], (8, 3));
    assert_eq!(
        check_walkable(&closed, &[flyer], &rules, corridor_body),
        Ok(())
    );
    assert_eq!(
        check_walkable(&closed, &both, &rules, corridor_body),
        Err(MapProblem::WaypointUnreachable {
            path: DeclaredName::new("lane").unwrap(),
            waypoint: 1,
        })
    );
    let mut clouded = corridor(&[(5, 0), (5, 4)], (8, 3));
    clouded.units.push(placed("cloud", (7, 3)));
    assert_eq!(
        check_walkable(&clouded, &[ground(half)], &rules, corridor_body),
        Ok(())
    );
    assert_eq!(
        check_walkable(&clouded, &both, &rules, corridor_body),
        blocked("camp")
    );
    assert_eq!(check_walkable(&clouded, &[], &rules, corridor_body), Ok(()));
}

#[test]
fn a_wall_closes_a_way_and_loads_only_as_a_simple_polygon_on_a_declared_layer() {
    let half = Num::HALF;
    let point = MapPoint::ground;
    let rules = ModeNavigationData::default();
    let flyer = Walker {
        layer: AIR,
        radius: half,
    };
    let both = [ground(half), flyer];
    // A wall on the ground across the map from x = 5 to 6 closes the lane as the towers do, and
    // the air walker flies over it; a wall round the spawn marker leaves the walker no place
    // there. A wall's points must be in the map's bounds, on a layer the mode declares, and make
    // a simple polygon.
    let wall = |layer: Option<&str>, points: &[(i64, i64)]| WallData {
        layer: layer.map(|layer| DeclaredName::new(layer).unwrap()),
        points: points.iter().map(|&(x, z)| point(x, z)).collect(),
    };
    let walled = |walls: Vec<WallData>| {
        let mut walled = corridor(&[], (8, 3));
        let navigation = walled.section_mut::<MapNavigationData>();
        navigation.unwrap().walls = walls;
        walled
    };
    let across = walled(vec![wall(None, &[(5, 0), (6, 0), (6, 4), (5, 4)])]);
    assert_eq!(
        check_walkable(&across, &[flyer], &rules, corridor_body),
        Ok(())
    );
    assert_eq!(
        check_walkable(&across, &both, &rules, corridor_body),
        Err(MapProblem::WaypointUnreachable {
            path: DeclaredName::new("lane").unwrap(),
            waypoint: 1,
        })
    );
    let spawning = ModeNavigationData {
        spawn_tags: vec![DeclaredName::new("spawn").unwrap()],
        ..ModeNavigationData::default()
    };
    let round = walled(vec![wall(None, &[(0, 0), (2, 0), (2, 2), (0, 2)])]);
    let spawn = DeclaredName::new("spawn").unwrap();
    let blocked = Err(MapProblem::MarkerBlocked { marker: spawn });
    assert_eq!(
        check_walkable(&round, &both, &spawning, corridor_body),
        blocked
    );
    let air = DeclaredName::new("air").unwrap();
    let layered = ModeNavigationData {
        layers: vec![DeclaredName::new("ground").unwrap(), air.clone()],
        spawn_tags: Vec::new(),
    };
    let refused = [
        (
            wall(Some("air"), &[(5, 0), (6, 0), (6, 4)]),
            &rules,
            NavigationDataError::UnknownLayer(air),
        ),
        (
            wall(None, &[(5, 0), (6, 0), (6, 5)]),
            &rules,
            NavigationDataError::Point(MapPointError::OutOfBounds),
        ),
        (
            wall(None, &[(5, 0), (6, 0)]),
            &rules,
            NavigationDataError::Wall {
                wall: 0,
                problem: PolygonError::TooFewPoints,
            },
        ),
        (
            wall(Some("air"), &[(5, 0), (6, 4), (6, 0), (5, 4)]),
            &layered,
            NavigationDataError::Wall {
                wall: 0,
                problem: PolygonError::EdgesMeet {
                    first: 0,
                    second: 2,
                },
            },
        ),
    ];
    let walls = |map: &MapData, rules| {
        let ground = MapBook::of(map.sections(), None).unwrap();
        Navigation::walls_of(&ground, map.sections(), Some(&layer_table(rules)))
    };
    for (wall, rules, error) in refused {
        assert_eq!(walls(&walled(vec![wall]), rules), Err(error));
    }
    let lifted = walled(vec![wall(Some("air"), &[(5, 0), (6, 0), (6, 4)])]);
    let walls = walls(&lifted, &layered).unwrap();
    assert_eq!(
        walls
            .walls()
            .iter()
            .map(|wall| wall.layer)
            .collect::<Vec<_>>(),
        [AIR]
    );
}

#[test]
fn a_placed_box_lies_within_the_bounds_and_may_overlap_the_walls_and_the_bodies_that_stand() {
    // Boxes of 2 by 2 m, a tower of 0.9 m at (5, 2), and a building from (6, 0) to (8, 1), in the
    // corridor from (0, 0) to (10, 4). A box at (3, 2) clears the tower; one at (4.2, 2) overlaps
    // it, and loads, as the bodies block alike (04-capabilities/navigation.md, Placed obstacles may
    // overlap). One at (7, 1.5) overlaps the building's wall, and loads too, as one at (7, 2.5)
    // does. One at (9.5, 2) reaches past the bounds, and is refused. Two side by side at (1, 2) and
    // (3, 2) touch, and two at (1, 2) and (2.5, 2) overlap: both load.
    let point = MapPoint::ground;
    let rules = ModeNavigationData::default();
    let body_of = |unit_type: &str| match unit_type {
        "crate" => BodyShape::box_sized([Num::int(2), Num::int(2)])
            .map(|shape| BodyForm::new(shape, Layer::FIRST)),
        other => corridor_body(other),
    };
    let check = |boxes: &[(Num, Num)], walled: bool| {
        let mut map = corridor(&[(5, 2)], (8, 3));
        map.section_mut::<MapPathsData>().unwrap().0.clear();
        map.markers.clear();
        if walled {
            let corner = |x, z| point(x, z);
            map = map.with(MapNavigationData {
                cell: Num::HALF,
                walls: vec![WallData {
                    layer: None,
                    points: vec![corner(6, 0), corner(8, 0), corner(8, 1), corner(6, 1)],
                }],
            });
        }
        let placed = boxes.iter().map(|&(x, z)| PlacedUnitData {
            position: MapPoint::Ground([x, z]),
            ..PlacedUnitData::new("crate", "west", point(0, 0))
        });
        map.units.extend(placed);
        check_walkable(&map, &[ground(Num::HALF)], &rules, body_of)
    };
    let blocked = |unit| {
        Err(MapProblem::BoxOutside {
            unit,
            unit_type: DeclaredName::new("crate").unwrap(),
        })
    };
    let num = |text: &str| text.parse::<Num>().unwrap();
    // The tower and the creep come first among the placed units, at 0 and 1.
    assert_eq!(check(&[(num("3"), num("2"))], false), Ok(()));
    assert_eq!(check(&[(num("4.2"), num("2"))], false), Ok(()));
    assert_eq!(check(&[(num("7"), num("1.5"))], true), Ok(()));
    assert_eq!(check(&[(num("7"), num("2.5"))], true), Ok(()));
    assert_eq!(check(&[(num("9.5"), num("2"))], false), blocked(2));
    assert_eq!(
        check(&[(num("1"), num("2")), (num("3"), num("2"))], false),
        Ok(())
    );
    assert_eq!(
        check(&[(num("1"), num("2")), (num("2.5"), num("2"))], false),
        Ok(())
    );
}
