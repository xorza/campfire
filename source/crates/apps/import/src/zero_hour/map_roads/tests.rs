use super::*;
use crate::zero_hour::format::ini::roads_ini::samples::ROADS;

/// The flags of a road's ends.
const START: u32 = 0x02;
const END: u32 = 0x04;
const ANGLED: u32 = 0x08;
const TIGHT: u32 = 0x40;
const JOIN: u32 = 0x80;

/// An object of `template` at `[x, y]`, of `flags`.
fn object(template: &str, [x, y]: [f32; 2], flags: u32) -> MapObject {
    MapObject {
        position: [x, y, 0.0],
        angle: 0.0,
        flags,
        template: template.as_bytes().to_vec(),
        waypoint: None,
    }
}

fn roads() -> RoadsIni {
    let mut roads = RoadsIni::default();
    roads.read(ROADS.as_bytes()).unwrap();
    roads
}

#[test]
fn a_start_and_the_end_after_it_are_a_segment_of_the_type_its_template_names() {
    let roads = roads();
    let [gravel, sidewalk] = [RoadId(0), RoadId(1)];
    let objects = [
        object("GravelRoad", [0.0, 0.0], START | ANGLED),
        object("GravelRoad", [40.0, 0.0], END | JOIN),
        // A start with no end after it is none, and the object after it is read again.
        object("Sidewalk", [5.0, 5.0], START),
        object("Rock", [5.0, 5.0], 0),
        // A start and an end at one point: the end moved 0.25 along `x`.
        object("Sidewalk", [10.0, 10.0], START | TIGHT),
        object("Sidewalk", [10.0, 10.0], END | JOIN),
        // A template no type names: its segment is dropped, and counted.
        object("Nope", [1.0, 1.0], START),
        object("Nope", [2.0, 2.0], END),
        object("GravelRoad", [40.0, 0.0], START),
        object("GravelRoad", [40.0, 30.0], END),
        object("GravelRoad", [100.0, 100.0], START),
        object("GravelRoad", [150.0, 100.0], END),
        object("GravelRoad", [100.0, 150.0], START),
        object("GravelRoad", [100.0, 100.0], END),
        object("GravelRoad", [-0.0, 100.0], START),
        object("GravelRoad", [100.0, 100.0], END),
        // A start at the list's end has no end.
        object("GravelRoad", [0.0, 0.0], START),
    ];
    let map = MapRoads::of(&objects, &roads);
    let end = |point: [f32; 2], angled: bool, join: bool| RoadEnd {
        point: Vec2::from(point),
        angled,
        join,
    };
    assert_eq!(
        map.segments[..3],
        [
            RoadSegment {
                road: gravel,
                start: end([0.0, 0.0], true, false),
                end: end([40.0, 0.0], false, true),
                tight: false,
            },
            RoadSegment {
                road: sidewalk,
                start: end([10.0, 10.0], false, false),
                end: end([10.25, 10.0], false, true),
                tight: true,
            },
            RoadSegment {
                road: gravel,
                start: end([40.0, 0.0], false, false),
                end: end([40.0, 30.0], false, false),
                tight: false,
            },
        ]
    );
    assert_eq!(map.segments.len(), 6);
    // The sidewalk, read last, draws first.
    assert_eq!(map.types(), [sidewalk, gravel]);
    assert_eq!(
        map.untyped().collect::<Vec<_>>(),
        [DroppedRoads {
            road: "Nope".to_owned(),
            cause: RoadDropCause::NoType,
            count: 1,
        }]
    );
}
