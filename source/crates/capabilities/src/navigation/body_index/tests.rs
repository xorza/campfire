use campfire_sim::IdAllocator;

use super::*;
fn body(id: StableId, x: i64, z: i64, radius: Num) -> IndexedBody {
    IndexedBody {
        id,
        at: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap(),
        radius,
        layer: Layer::FIRST,
    }
}

fn near_on(index: &BodyIndex, layer: Layer, x: i64, z: i64, reach: Num) -> Vec<StableId> {
    let mut found = Vec::new();
    index.near(
        layer,
        Vec3::new(Num::int(x), Num::ZERO, Num::int(z)),
        reach,
        |body| {
            found.push(body.id);
        },
    );
    found
}

fn near(index: &BodyIndex, x: i64, z: i64, reach: Num) -> Vec<StableId> {
    near_on(index, Layer::FIRST, x, z, reach)
}

#[test]
fn the_index_finds_each_body_near_once_and_follows_its_changes() {
    // Buckets of 2 m, for walkers of 1 m.
    let mut index = BodyIndex::new(Num::ONE);
    let mut ids = IdAllocator::default();
    let (wide, small, far) = (ids.allocate(), ids.allocate(), ids.allocate());
    // A body of 5 m at the origin covers buckets −3 to 2 on both axes, 36 of them; one of
    // 1 m at (3, 3) covers buckets 1 to 2, 4 of them; one at (40, 0), buckets 19 to 20 along x
    // and −1 to 0 along z.
    let bodies = [
        body(wide, 0, 0, Num::int(5)),
        body(small, 3, 3, Num::ONE),
        body(far, 40, 0, Num::ONE),
    ];
    assert!(index.update(&bodies));
    assert_eq!(index.entries.len(), 36 + 4 + 4);
    assert_eq!(index.added(), bodies);
    assert!(!index.update(&bodies));
    assert_eq!(index.added(), []);

    // Around (3, 3) by 1 m, buckets 1 to 2: the wide body and the small one share all four,
    // and each is found once. Around (0, 0) by 1 m, buckets −1 to 0: only the wide body. The
    // box of (40, 0) reaches no bucket between. Around (20, 0) by 20 m, rows −10 to 10, each
    // is found in its first row there: the wide body in row −3, the far one in −1, the small
    // one in 1.
    assert_eq!(near(&index, 3, 3, Num::ONE), [wide, small]);
    assert_eq!(near(&index, 0, 0, Num::ONE), [wide]);
    assert_eq!(near(&index, 20, 0, Num::ONE), []);
    assert_eq!(near(&index, 20, 0, Num::int(20)), [wide, far, small]);

    // The wide body moves to (40, 20): it is taken away and put in again; the small body
    // goes, and a new one comes.
    let new = ids.allocate();
    let moved = [
        body(wide, 40, 20, Num::int(5)),
        body(far, 40, 0, Num::ONE),
        body(new, -10, -10, Num::ONE),
    ];
    assert!(index.update(&moved));
    assert_eq!(index.removed(), [bodies[0], bodies[1]]);
    assert_eq!(index.added(), [moved[0], moved[2]]);
    assert_eq!(near(&index, 3, 3, Num::ONE), []);
    assert_eq!(near(&index, 40, 16, Num::ONE), [wide]);
    assert_eq!(near(&index, -10, -10, Num::ONE), [new]);

    // The same bodies put in at once give the same entries.
    let mut again = BodyIndex::new(Num::ONE);
    again.update(&moved);
    assert_eq!(again.entries, index.entries);

    // A body of another layer at (−10, −10), and one beside it at (−9, −10): each layer's
    // query finds only its own, and a walker of 1 m along x through both meets only its own.
    let (air, above) = (Layer::new(1), ids.allocate());
    let layered = [
        moved[0],
        moved[1],
        moved[2],
        IndexedBody {
            layer: air,
            ..body(above, -9, -10, Num::ONE)
        },
    ];
    assert!(index.update(&layered));
    assert_eq!(near(&index, -10, -10, Num::ONE), [new]);
    assert_eq!(near_on(&index, air, -10, -10, Num::ONE), [above]);
    assert_eq!(near_on(&index, Layer::new(2), -10, -10, Num::ONE), []);
    let at = |x| Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(-13))).unwrap();
    let (beside, past) = (Segment::new(at(-12), at(-7)), Segment::new(at(30), at(31)));
    for (layer, blocked) in [(Layer::FIRST, true), (air, true), (Layer::new(2), false)] {
        let walker = Walker {
            layer,
            radius: Num::int(3),
        };
        // 3 m from each center to the segment along z = −13: within the radii's 1 + 3 = 4 m.
        assert_eq!(index.blocks(beside, walker), blocked, "{layer:?}");
        assert!(!index.blocks(past, walker));
    }
    assert!(index.update(&moved));

    assert!(index.update(&[]));
    assert_eq!(index.entries, []);
    assert_eq!(index.removed(), moved);
}

#[test]
fn a_search_meets_exactly_the_bodies_whose_buckets_it_covers() {
    // Buckets of 2 m, for walkers of 1 m.
    let mut index = BodyIndex::new(Num::ONE);
    let mut ids = IdAllocator::default();
    // A lattice of bodies of 0 to 5 m, 3 m apart, searched around many points: each body
    // whose buckets meet the search's on both axes is met once, and no other; and a body
    // blocks a segment exactly when one of them comes within reach of it.
    let mut bodies = Vec::new();
    for row in -6_i64..=6 {
        for column in -6_i64..=6 {
            let radius = Num::from_bits((row * 7 + column * 3).rem_euclid(11) << 23);
            bodies.push(body(ids.allocate(), column * 3, row * 3, radius));
        }
    }
    assert!(index.update(&bodies));
    let bucket = index.bucket;
    for (x, z, reach) in [(0, 0, 1), (5, -7, 3), (-17, 11, 0), (13, 13, 6), (2, 19, 2)] {
        let reach = Num::int(reach);
        let mut met = near(&index, x, z, reach);
        let count = met.len();
        met.sort_unstable();
        met.dedup();
        assert_eq!(met.len(), count, "each body once");
        let meets =
            |own: Buckets, search: Buckets| own.low <= search.high && search.low <= own.high;
        let expected: Vec<StableId> = bodies
            .iter()
            .filter(|body| {
                let at = body.at.get();
                let rows = BodyIndex::buckets(bucket, Num::int(z), reach);
                let columns = BodyIndex::buckets(bucket, Num::int(x), reach);
                meets(BodyIndex::buckets(bucket, at.z, body.radius), rows)
                    && meets(BodyIndex::buckets(bucket, at.x, body.radius), columns)
            })
            .map(|body| body.id)
            .collect();
        assert_eq!(met, expected, "({x}, {z})");
        let point =
            |x: i64, z: i64| Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap();
        let segment = Segment::new(point(x, z), point(x + 4, z - 2));
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::ONE,
        };
        let reached = bodies
            .iter()
            .any(|body| segment.comes_within(body.at, walker.radius + body.radius));
        assert_eq!(index.blocks(segment, walker), reached, "({x}, {z})");
    }
}
