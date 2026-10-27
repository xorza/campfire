use super::*;

/// A ground that rises 1 m for each metre along `z`.
fn slope(_: f32, z: f32) -> f32 {
    z
}

const fn seconds(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

#[test]
fn a_track_lays_its_edges_across_its_move_fades_them_and_drops_them() {
    // An edge each 10 m, 2 m over the ground, up to 4 edges, the newest 2 opaque, each fading
    // over 8 s; one track at once, 4 m wide.
    let marks = TrackMarks {
        spacing: 10.0,
        lift: 2.0,
        edges: 4,
        opaque: 2,
        fade: 8.0,
        tracks: 1,
    };
    let mut run = TrackRun::new(marks, 1);
    let [tank, other] = [1, 2].map(|raw| Entity::from_raw_u32(raw).unwrap());
    run.bind(tank, 0, 4.0);
    run.bind(other, 0, 4.0);
    assert_eq!(run.tracks.len(), 1);
    let lay = |run: &mut TrackRun, millis, x: Option<f32>| {
        run.lay(seconds(millis), &slope, |_| {
            x.map(|x| Vec3::new(x, 0.0, 0.0))
        });
    };
    // The first place anchors it; 5 m on lays none; 10 m on, an edge across its move along `x`:
    // its ends 2 m to either side along `z`, each 2 m over the ground under it, −2 and 2; its
    // `v` 1, transparent as it starts the track.
    lay(&mut run, 0, Some(0.0));
    lay(&mut run, 500, Some(5.0));
    assert!(run.tracks[0].edges.is_empty());
    lay(&mut run, 1000, Some(10.0));
    let edge = |x: f32, v, born, alpha| Edge {
        ends: [Vec3::new(x, 0.0, -2.0), Vec3::new(x, 4.0, 2.0)],
        v,
        born: seconds(born),
        alpha,
    };
    assert_eq!(run.tracks[0].edges, [edge(10.0, 1.0, 1000, 0.0)]);
    // The second edge lies transparent too, the third and the fourth opaque, each fading by its
    // age: at 4 s, the one of 3 s by 1/8.
    for (millis, x) in [(2000, 20.0), (3000, 30.0), (4000, 40.0)] {
        lay(&mut run, millis, Some(x));
    }
    assert_eq!(
        run.tracks[0].edges,
        [
            edge(10.0, 1.0, 1000, 0.0),
            edge(20.0, 0.0, 2000, 0.0),
            edge(30.0, 1.0, 3000, 0.875),
            edge(40.0, 0.0, 4000, 1.0),
        ]
    );
    // Its quads: the two oldest past the 2 opaque fade by their place, 1 − 2/2 and 1 − 1/2, each
    // times its alpha of 0; then 0.875 and 1. Three quads, counter-clockwise from above.
    run.build();
    let quads = run.group(0);
    let alphas: Vec<f32> = quads.colors.iter().map(|color| color[3]).collect();
    assert_eq!(alphas, [0.0, 0.0, 0.0, 0.0, 0.875, 0.875, 1.0, 1.0]);
    assert_eq!(quads.positions[1], [10.0, 4.0, 2.0]);
    assert_eq!(
        quads.uvs[..4],
        [[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]]
    );
    assert_eq!(
        quads.indices,
        [0, 1, 2, 2, 1, 3, 2, 3, 4, 4, 3, 5, 4, 5, 6, 6, 5, 7]
    );
    let [a, b, c] = [0, 1, 2].map(|at| Vec3::from(quads.positions[at]));
    assert!((b - a).cross(c - a).y > 0.0);
    // A fifth edge drops the oldest.
    lay(&mut run, 5000, Some(50.0));
    let borns: Vec<Duration> = run.tracks[0].edges.iter().map(|edge| edge.born).collect();
    assert_eq!(borns, [2000, 3000, 4000, 5000].map(seconds));
    // Its drawing gone, the track lays no more: at 10.5 s the edge of 2 s faded out and went, and
    // the one of 3 s fades to 1 − 7.5/8; at 13 s every edge went, and the track with them, so
    // another drawing binds one.
    lay(&mut run, 6000, None);
    lay(&mut run, 10_500, Some(60.0));
    let edges = &run.tracks[0].edges;
    assert_eq!((edges.len(), edges[0].alpha), (3, 0.0625));
    lay(&mut run, 13_000, None);
    assert!(run.tracks.is_empty());
    run.bind(other, 0, 4.0);
    assert_eq!(run.tracks.len(), 1);
}
