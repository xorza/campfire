use campfire_client_files::EffectsFile;
use campfire_codec::Toml;
use campfire_common::DeclaredName;
use campfire_units::TypeOrigin;

use bevy::asset::AssetPath;
use bevy::math::Vec3;
use campfire_client_files::WreckLaunch;

use super::*;
use crate::view::effects::wreck_run::WreckEnd;

#[test]
fn a_type_plays_the_death_end_and_trail_its_look_names_of_those_the_book_holds() {
    // The book holds the blast and the smoke; the bunker's death names the blast at its start
    // and its end, and a list the book lacks at its end too, which it plays none of; the tank
    // names a death of no list, so it plays none.
    let effects: EffectsFile = Toml::parse(
        r#"
        step = 0.033333335
        cap = 10
        [systems.smoke]
        blend = { mode = "alpha" }
        texture = "client/textures/smoke.ktx2"
        ground_aligned = false
        up_towards_emitter = false
        above_ground_only = false
        angle = [0.0, 0.0]
        angular_rate = [0.0, 0.0]
        angular_damping = [1.0, 1.0]
        velocity_damping = [1.0, 1.0]
        gravity = 0.0
        drift = [0.0, 0.0, 0.0]
        lifetime = [10.0, 10.0]
        system_lifetime = 1
        size = [1.0, 1.0]
        start_size_rate = [0.0, 0.0]
        size_rate = [0.0, 0.0]
        size_rate_damping = [1.0, 1.0]
        alpha = [{ alpha = [1.0, 1.0], at = 0 }]
        color = [{ color = [1.0, 1.0, 1.0], at = 0 }]
        color_scale = [0.0, 0.0]
        burst_delay = [0.0, 0.0]
        burst_count = [1.0, 1.0]
        initial_delay = [0.0, 0.0]
        velocity = { kind = "still" }
        volume = { kind = "point" }
        hollow = false
        [[lists.blast.systems]]
        system = "smoke"
        count = 1
        offset = [0.0, 0.0, 0.0]
        radius = [0.0, 0.0]
        height = [0.0, 0.0]
        rotation = [0.0, 0.0, 0.0, 1.0]
        orient_to_object = false
        attach_to_object = false
        at_ground = false
        callers_radius = false
        ricochet = false
        "#,
    )
    .unwrap();
    let looks: ClientUnits = Toml::parse(
        r#"
        [units.bunker]
        draws = []
        death = { start = ["blast"], end = ["fizzle", "blast"], wrecks = { end = [[{ models = ["client/models/hulk.glb"], count = 1, offset = ["0", "0", "0"], launch = "still", spin = "0", sink = { delay = "1", rate = "1", gone = "2" } }]] } }
        [units.tank]
        draws = [{ states = [
            { sets = [[]], models = [] },
            { sets = [["damaged"]], models = [], fire_bones = [{ slot = 1, barrels = [{ bone = "MuzzleFX01" }] }] },
        ] }]
        death = { start = ["fizzle"] }
        tread = { systems = ["smoke", "dirt"], track = { texture = "client/tracks/tank.ktx2", width = "14.5" } }
        chassis = { pitch_stiffness = "0.0625", roll_stiffness = "0.03125", pitch_damping = "0.75", roll_damping = "0.25", axial_damping = "1", forward_pitch = "0.5", lateral_roll = "0.25", accelerating = "0.0625", braking = "0.125" }
        [units.jeep]
        draws = []
        tread = { track = { texture = "client/tracks/tire.ktx2", width = "6" } }
        [units.truck]
        draws = []
        tread = { track = { texture = "client/tracks/tire.ktx2", width = "8" } }
        [units.shell]
        draws = []
        end = "blast"
        trail = "smoke"
        [conditions]
        damaged = { test = "map" }
        "#,
    )
    .unwrap();
    let book = EffectBook::new(&effects);
    let origins: TypeOriginBook = ["bunker", "tank", "shell", "jeep", "truck"]
        .map(|name| TypeOrigin {
            package: PackageIndex::MODE,
            name: name.into(),
        })
        .into_iter()
        .collect();
    let plays = EffectPlays::of(PlaySources {
        book: &book,
        looks: &looks,
        origins: &origins,
        names: &MatchNames::default(),
    });
    let [bunker, tank, shell, jeep, truck] =
        [0, 1, 2, 3, 4].map(|at| origins.iter().nth(at).unwrap().0);
    let blast = book.list_id(&DeclaredName::new("blast").unwrap()).unwrap();
    let smoke = book.id(&DeclaredName::new("smoke").unwrap()).unwrap();
    // Its end throws its hulk, of the mode's model's scene.
    let hulk = WreckPlay {
        scenes: vec![AssetPath::from("package://0/client/models/hulk.glb#Scene0")],
        count: 1,
        offset: Vec3::ZERO,
        launch: WreckLaunch::Still,
        spin: 0.0,
        end: WreckEnd {
            life: None,
            delay: 1.0,
            rate: 1.0,
            gone: 2.0,
        },
    };
    let bunker_death = DeathPlays {
        start: vec![blast],
        middle: Vec::new(),
        end: vec![blast],
        wrecks: [Vec::new(), Vec::new(), vec![vec![hulk]]],
    };
    assert_eq!(plays.deaths, BTreeMap::from([(bunker, bunker_death)]));
    assert_eq!(plays.ends, BTreeMap::from([(shell, blast)]));
    assert_eq!(plays.trails, BTreeMap::from([(shell, smoke)]));
    assert!(plays.fires.is_empty() && plays.hits.is_empty());
    // The tank's treads run the smoke, not the dirt the book lacks, and lay its track; the jeep
    // and the truck lay theirs of one texture, so of one group.
    let track = |group, width| TrackPlay { group, width };
    let laid = |group, width| TreadPlay {
        systems: Vec::new(),
        track: Some(track(group, width)),
    };
    assert_eq!(
        plays.treads,
        BTreeMap::from([
            (
                tank,
                TreadPlay {
                    systems: vec![smoke],
                    track: Some(track(0, 14.5)),
                }
            ),
            (jeep, laid(1, 6.0)),
            (truck, laid(1, 8.0)),
        ])
    );
    // The tank's chassis alone, its springs in floats, each exact in binary.
    assert_eq!(
        plays.chassis,
        BTreeMap::from([(
            tank,
            ChassisPlay {
                pitch_stiffness: 0.0625,
                roll_stiffness: 0.03125,
                pitch_damping: 0.75,
                roll_damping: 0.25,
                axial_damping: 1.0,
                forward_pitch: 0.5,
                lateral_roll: 0.25,
                accelerating: 0.0625,
                braking: 0.125,
            }
        )])
    );
    let texture = |path| PackagePath::parse(path).unwrap();
    assert_eq!(
        plays.track_textures,
        [
            texture("client/tracks/tank.ktx2"),
            texture("client/tracks/tire.ktx2")
        ]
    );
}
