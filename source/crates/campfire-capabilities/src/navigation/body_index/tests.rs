use campfire_sim::IdAllocator;

use super::*;
fn body(id: StableId, x: i64, z: i64, radius: Num) -> IndexedBody {
    IndexedBody {
        id,
        at: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap(),
        shape: Shape::Circle(radius),
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
    // Bucket keys order as their layer, row and column do, both signs; a row past the keys'
    // range clamps to its end.
    let rows = BucketKey::ROWS;
    let key = |layer: u8, row: i64, column: i64| BucketKey::new(Layer::new(layer), row, column);
    let ordered = [
        key(0, -rows, 0),
        key(0, -rows, 5),
        key(0, -1, i64::MAX),
        key(0, 0, i64::MIN),
        key(0, 0, -1),
        key(0, 0, 0),
        key(0, 1, -5),
        key(0, rows - 1, 0),
        key(1, -rows, i64::MIN),
        key(1, 0, 0),
    ];
    assert!(ordered.is_sorted_by(|a, b| a < b));
    assert_eq!(key(0, i64::MIN, 5), key(0, -rows, 5));
    assert_eq!(key(0, i64::MAX, 0), key(0, rows - 1, 0));

    // A lattice searched around many points: each body whose buckets meet the search's on both
    // axes is met once, and no other; and a body blocks a segment exactly when one of them comes
    // within reach of it.
    let mut ids = IdAllocator::default();
    let (index, bodies) = lattice(&mut ids);
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
                meets(BodyIndex::buckets(bucket, at.z, body.shape.bound()), rows)
                    && meets(
                        BodyIndex::buckets(bucket, at.x, body.shape.bound()),
                        columns,
                    )
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
            .any(|body| segment.comes_within(body.at, walker.radius + body.shape.bound()));
        assert_eq!(index.blocks(segment, walker), reached, "({x}, {z})");
    }
}

/// Buckets of 2 m, for walkers of 1 m, over a lattice of bodies of 0 to 5 m, 3 m apart.
fn lattice(ids: &mut IdAllocator) -> (BodyIndex, Vec<IndexedBody>) {
    let mut index = BodyIndex::new(Num::ONE);
    let mut bodies = Vec::new();
    for row in -6_i64..=6 {
        for column in -6_i64..=6 {
            let radius = Num::from_bits((row * 7 + column * 3).rem_euclid(11) << 23);
            bodies.push(body(ids.allocate(), column * 3, row * 3, radius));
        }
    }
    assert!(index.update(&bodies));
    (index, bodies)
}

#[test]
fn a_segment_is_blocked_exactly_when_a_body_comes_within_reach() {
    let mut ids = IdAllocator::default();
    let (index, bodies) = lattice(&mut ids);
    // Segments of every direction and length over the lattice, long diagonals, lines along an
    // axis and single points among them, with ends off the whole meters, for walkers up to the
    // index's widest: one blocks exactly when a body comes within reach of it.
    let mut state = 0xB10C_u64;
    let mut next = move |span: u64| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) % span
    };
    let quarter = Num::ONE.to_bits() / 4;
    // And small bodies, of 0 to a half meter, scattered on the quarter meters, so a near miss
    // falls on either side of a bucket's edge.
    let mut fine = BodyIndex::new(Num::ONE);
    let mut scattered = Vec::new();
    for _ in 0..400 {
        let mut along = || Num::from_bits((next(161).cast_signed() - 80) * quarter);
        let at = Vec3::new(along(), Num::ZERO, along());
        scattered.push(IndexedBody {
            id: ids.allocate(),
            at: Position::new(at).unwrap(),
            shape: Shape::Circle(Num::from_bits(next(3).cast_signed() * quarter)),
            layer: Layer::FIRST,
        });
    }
    assert!(fine.update(&scattered));
    for case in 0..6000 {
        let (index, bodies) = if case % 2 == 0 {
            (&index, &bodies)
        } else {
            (&fine, &scattered)
        };
        let mut along = || Num::from_bits((next(161).cast_signed() - 80) * quarter);
        let from = Vec3::new(along(), Num::ZERO, along());
        let to = match case % 6 {
            0 => from,
            1 => Vec3::new(from.x, Num::ZERO, along()),
            2 => Vec3::new(along(), Num::ZERO, from.z),
            _ => Vec3::new(along(), Num::ZERO, along()),
        };
        let segment = Segment::new(Position::new(from).unwrap(), Position::new(to).unwrap());
        let walker = Walker {
            layer: Layer::FIRST,
            radius: Num::from_bits((next(4).cast_signed() + 1) * quarter),
        };
        let reached = bodies
            .iter()
            .any(|body| segment.comes_within(body.at, walker.radius + body.shape.bound()));
        assert_eq!(index.blocks(segment, walker), reached, "{from:?} {to:?}");
    }
}

#[test]
fn a_layer_whose_rows_spread_wide_still_meets_exactly_its_bodies() {
    // On the first layer two bodies 2¹⁸ m apart along z, rows far wider than its entries, so
    // the layer keeps no row starts; on the second, a body near each. Each search meets the
    // bodies of its layer near it, and a segment along z meets those of its layer within reach.
    let mut ids = IdAllocator::default();
    let far = 1 << 18;
    let mut on = |layer: u8, z: i64| IndexedBody {
        layer: Layer::new(layer),
        ..body(ids.allocate(), 0, z, Num::ONE)
    };
    let bodies = [on(0, 0), on(1, 2), on(0, far), on(1, far - 2)];
    let mut index = BodyIndex::new(Num::ONE);
    assert!(index.update(&bodies));
    assert_eq!(index.rows.span(Layer::FIRST), Some((-1, far / 2)));
    for (layer, z, expected) in [
        (0, 0, vec![bodies[0].id]),
        (1, 0, vec![bodies[1].id]),
        (0, far, vec![bodies[2].id]),
        (1, far, vec![bodies[3].id]),
        (0, far / 2, vec![]),
    ] {
        let met = near_on(&index, Layer::new(layer), 0, z, Num::int(3));
        assert_eq!(met, expected, "layer {layer} at {z}");
    }
    let point =
        |x: i64, z: i64| Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap();
    let walker = |layer: u8| Walker {
        layer: Layer::new(layer),
        radius: Num::ONE,
    };
    // 4 m off the bodies' line, past their 1 m and the walker's 1 m; 1 m off, within them.
    assert!(!index.blocks(Segment::new(point(4, 0), point(4, far)), walker(0)));
    assert!(index.blocks(Segment::new(point(1, 0), point(1, far)), walker(0)));
    assert!(!index.blocks(Segment::new(point(0, 10), point(0, 20)), walker(1)));
}
