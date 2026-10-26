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
        // A template no type names: the type of id 1, the gravel road, 8 wide and all its
        // texture across.
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
    // A segment takes its type's width and share of it across, the gravel road's 35 and 0.9 and
    // the sidewalk's 24 and 0.95.
    assert_eq!(
        map.segments[..4],
        [
            RoadSegment {
                road: gravel,
                width: 35.0,
                width_in_texture: 0.9,
                start: end([0.0, 0.0], true, false),
                end: end([40.0, 0.0], false, true),
                tight: false,
            },
            RoadSegment {
                road: sidewalk,
                width: 24.0,
                width_in_texture: 0.95,
                start: end([10.0, 10.0], false, false),
                end: end([10.25, 10.0], false, true),
                tight: true,
            },
            RoadSegment {
                road: gravel,
                width: 8.0,
                width_in_texture: 1.0,
                start: end([1.0, 1.0], false, false),
                end: end([2.0, 2.0], false, false),
                tight: false,
            },
            RoadSegment {
                road: gravel,
                width: 35.0,
                width_in_texture: 0.9,
                start: end([40.0, 0.0], false, false),
                end: end([40.0, 30.0], false, false),
                tight: false,
            },
        ]
    );
    assert_eq!(map.segments.len(), 7);
    // The sidewalk, read last, draws first.
    assert_eq!(map.types(), [sidewalk, gravel]);
    assert_eq!(map.untyped().count(), 0);

    // Where the type of id 1 is a bridge, a segment of no type is dropped, and counted, as the
    // sidewalk is, of these types none.
    let mut bridge_first = RoadsIni::default();
    bridge_first
        .read(b"Bridge B\nEnd\nRoad GravelRoad\nEnd\n")
        .unwrap();
    let map = MapRoads::of(&objects, &bridge_first);
    assert_eq!(map.segments.len(), 5);
    let dropped = |road: &str| DroppedRoads {
        road: road.to_owned(),
        cause: RoadDropCause::NoType,
        count: 1,
    };
    assert_eq!(
        map.untyped().collect::<Vec<_>>(),
        [dropped("Nope"), dropped("Sidewalk")]
    );
}
