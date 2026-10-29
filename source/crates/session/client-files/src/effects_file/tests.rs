use std::num::NonZeroU16;

use campfire_codec::Toml;

use super::*;
use crate::sound_event::{Hearers, SoundLimit, SoundPick, SoundRange};
use crate::spread::Spread;
use crate::tread_dust::SpeedScale;

/// A file of a smoke system with a spark slave and a list that starts it twice.
const SMOKE: &str = r#"
    step = 0.033333335
    cap = 2500

    [systems.smoke]
    rank = 3
    blend = { mode = "alpha" }
    texture = "client/textures/art/textures/exsmoke.ktx2"
    ground_aligned = false
    up_towards_emitter = false
    above_ground_only = false
    angle = [0.0, 6.2831855]
    angular_rate = [-0.01, 0.01]
    angular_damping = [1.0, 1.0]
    velocity_damping = [0.95, 0.95]
    gravity = -0.01
    drift = [0.0, 0.1, 0.0]
    lifetime = [30.0, 45.0]
    system_lifetime = 0
    size = [1.0, 2.0]
    start_size_rate = [0.0, 0.0]
    size_rate = [0.5, 0.5]
    size_rate_damping = [0.98, 0.98]
    alpha = [{ alpha = [0.0, 0.0], at = 0 }, { alpha = [0.5, 0.75], at = 5 }, { alpha = [0.0, 0.0], at = 30 }]
    color = [{ color = [1.0, 1.0, 1.0], at = 0 }]
    color_scale = [0.0, 0.0]
    burst_delay = [1.0, 1.0]
    burst_count = [1.0, 2.0]
    initial_delay = [0.0, 0.0]
    velocity = { kind = "ortho", x = [0.0, 0.0], y = [0.5, 1.0], z = [0.0, 0.0] }
    volume = { kind = "cylinder", radius = 2.0, length = 0.0 }
    hollow = true
    slave = { system = "spark", offset = [0.0, 1.0, 0.0] }

    [systems.spark]
    blend = { mode = "add" }
    texture = "client/textures/art/textures/exspark.ktx2"
    ground_aligned = false
    up_towards_emitter = false
    above_ground_only = false
    angle = [0.0, 0.0]
    angular_rate = [0.0, 0.0]
    angular_damping = [0.0, 0.0]
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
    color = [{ color = [1.0, 0.5, 0.0], at = 0 }, { color = [0.0, 0.0, 0.0], at = 10 }]
    color_scale = [0.0, 0.0]
    burst_delay = [0.0, 0.0]
    burst_count = [8.0, 8.0]
    initial_delay = [0.0, 0.0]
    velocity = { kind = "spherical", speed = [1.0, 2.0] }
    volume = { kind = "point" }
    hollow = false

    [[lists.tank_hit.systems]]
    system = "smoke"
    count = 2
    offset = [0.0, 0.0, 0.0]
    radius = [0.0, 3.0]
    height = [0.0, 0.0]
    delay = [3.0, 6.0]
    rotation = [0.0, 0.0, 0.0, 1.0]
    orient_to_object = false
    attach_to_object = false
    at_ground = true
    callers_radius = false
    ricochet = false
"#;

#[test]
fn effects_write_and_read_back_and_a_name_of_no_system_is_refused() {
    let file = EffectsFile::decode(SMOKE.as_bytes()).unwrap();
    assert_eq!(file.cap(), 2500);
    assert_eq!(file.step(), 1.0 / 30.0);
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let smoke = &file.systems()[&name("smoke")];
    assert_eq!(smoke.rank, Some(3));
    assert_eq!(
        smoke.alpha[1].alpha,
        Spread {
            low: 0.5,
            high: 0.75
        }
    );
    assert_eq!(
        smoke.slave.as_ref().map(|slave| &slave.system),
        Some(&name("spark"))
    );
    let spark = &file.systems()[&name("spark")];
    assert_eq!(spark.rank, None);
    let entry = &file.lists()[&name("tank_hit")].systems[0];
    assert_eq!(
        (entry.count, entry.delay.map(|delay| delay.high)),
        (2, Some(6.0))
    );
    assert_eq!(EffectsFile::decode(&file.encode()).unwrap(), file);

    // Each name of no system is refused: a list's, a slave's, a particle's system.
    let refused = |from: &str, to: &str| {
        let text = SMOKE.replacen(from, to, 1);
        Toml::parse::<EffectsFile>(&text).unwrap_err().to_string()
    };
    assert!(refused("system = \"smoke\"", "system = \"fire\"").contains("`fire`"));
    assert!(refused("system = \"spark\"", "system = \"flash\"").contains("`flash`"));
    let attached = refused("hollow = true", "hollow = true\n    attached = \"ember\"");
    assert!(attached.contains("`ember`"));
    // Keys out of order, or of one step twice, a key list empty, a number not finite and a step
    // of no length are refused.
    assert!(refused("at = 5", "at = 0").contains("`smoke`"));
    assert!(refused("at = 30", "at = 5").contains("`smoke`"));
    let empty = refused("alpha = [{ alpha = [1.0, 1.0], at = 0 }]", "alpha = []");
    assert!(empty.contains("`spark`"));
    assert!(refused("gravity = -0.01", "gravity = nan").contains("`smoke`"));
    assert!(refused("radius = [0.0, 3.0]", "radius = [0.0, inf]").contains("`tank_hit`"));
    assert!(refused("step = 0.033333335", "step = 0.0").contains("step"));
    // A file names how its wrecks meet the ground, none when it does not; one whose gravity
    // pulls up, or whose share passes 1, is refused.
    assert!(file.wrecks().is_none());
    let ground = "step = 0.033333335\nwrecks = { gravity = -64.0, stiffness = 0.8, friction = 0.15, rest = 15.0 }";
    let falls = SMOKE.replacen("step = 0.033333335", ground, 1);
    let falls = EffectsFile::decode(falls.as_bytes()).unwrap();
    assert_eq!(
        falls.wrecks(),
        WreckGround {
            gravity: -64.0,
            stiffness: 0.8,
            friction: 0.15,
            rest: 15.0,
        }
    );
    assert_eq!(EffectsFile::decode(&falls.encode()).unwrap(), falls);
    let up = refused("step = 0.033333335", &ground.replace("-64.0", "1.0"));
    assert!(up.contains("ground"), "{up}");
    let past = refused("step = 0.033333335", &ground.replace("0.8", "1.5"));
    assert!(past.contains("ground"), "{past}");
    // A file names how its treads throw their dust and lay their tracks, none when it does not;
    // a scale below 0, a track of one edge or more opaque edges than all are refused.
    assert!(file.treads().is_none() && file.tracks().is_none());
    let treads = "step = 0.033333335\n\
                  treads = { moving = 0.1, across = { base = 0.1, per_speed = 0.0167, most = 1.0 }, \
                  rise = { base = 0.1, per_speed = 0.0333, most = 1.0 }, \
                  count = { base = 0.1, per_speed = 0.0333, most = 1.0 } }\n\
                  tracks = { spacing = 10.0, lift = 2.0, edges = 100, opaque = 25, fade = 300.0, tracks = 100 }";
    let driven = SMOKE.replacen("step = 0.033333335", treads, 1);
    let driven = EffectsFile::decode(driven.as_bytes()).unwrap();
    let scale = |per_speed| SpeedScale {
        base: 0.1,
        per_speed,
        most: 1.0,
    };
    assert_eq!(
        driven.treads(),
        TreadDust {
            moving: 0.1,
            across: scale(0.0167),
            rise: scale(0.0333),
            count: scale(0.0333),
        }
    );
    assert_eq!(
        driven.tracks(),
        TrackMarks {
            spacing: 10.0,
            lift: 2.0,
            edges: 100,
            opaque: 25,
            fade: 300.0,
            tracks: 100,
        }
    );
    assert_eq!(EffectsFile::decode(&driven.encode()).unwrap(), driven);
    // At 16 m a second, 0.125 + 16/32 = 0.625; at 64, 2.125, past its most of 1.
    let exact = SpeedScale {
        base: 0.125,
        per_speed: 1.0 / 32.0,
        most: 1.0,
    };
    assert_eq!([exact.at(16.0), exact.at(64.0)], [0.625, 1.0]);
    let below = refused("step = 0.033333335", &treads.replacen("0.1", "-0.1", 1));
    assert!(below.contains("dust"), "{below}");
    let single = refused(
        "step = 0.033333335",
        &treads.replace("edges = 100", "edges = 1"),
    );
    assert!(single.contains("tracks"), "{single}");
    let opaque = refused(
        "step = 0.033333335",
        &treads.replace("opaque = 25", "opaque = 101"),
    );
    assert!(opaque.contains("tracks"), "{opaque}");
}

#[test]
fn a_list_plays_its_sounds_and_a_sound_that_does_not_hold_or_is_not_held_is_refused() {
    // The tank's hit plays its smoke and a sound of three files drawn in turn, 60 % loud, losing
    // up to 15 % of it a play, 2 % lower to 2 % higher, at most 3 at once, the fourth stopping the
    // oldest, full to 175 m and silent at 800, for everyone.
    let sound = r#"
        [sounds.tank_boom]
        files = ["client/sounds/a.wav", "client/sounds/b.wav", "client/sounds/c.wav"]
        pick = "in_turn"
        volume = 0.6
        loss = 0.15
        pitch = [0.98, 1.02]
        limit = { count = 3, interrupt = true }
        range = { full = 175.0, silent = 800.0 }
        heard_by = "everyone"

        [sounds.click]
        files = ["client/sounds/click.wav"]
        volume = 1.0
        heard_by = "owner"
    "#;
    let text = format!("{SMOKE}\n    [lists.tank_hit]\n    sounds = [\"tank_boom\"]\n{sound}");
    let file = EffectsFile::decode(text.as_bytes()).unwrap();
    let name = |text: &str| DeclaredName::new(text).unwrap();
    assert_eq!(file.lists()[&name("tank_hit")].sounds, [name("tank_boom")]);
    let path = |text: &str| PackagePath::parse(text).unwrap();
    assert_eq!(
        file.sounds()[&name("tank_boom")],
        SoundEvent {
            files: vec![
                path("client/sounds/a.wav"),
                path("client/sounds/b.wav"),
                path("client/sounds/c.wav"),
            ],
            pick: SoundPick::InTurn,
            volume: 0.6,
            loss: 0.15,
            pitch: [0.98, 1.02],
            limit: Some(SoundLimit {
                count: NonZeroU16::new(3).unwrap(),
                interrupt: true,
            }),
            range: Some(SoundRange {
                full: 175.0,
                silent: 800.0,
            }),
            heard_by: Hearers::Everyone,
        }
    );
    // A sound of no field it may leave out draws at random, loses nothing, plays at its own
    // pitch with no limit, and flat.
    assert_eq!(
        file.sounds()[&name("click")],
        SoundEvent {
            files: vec![path("client/sounds/click.wav")],
            pick: SoundPick::Random,
            volume: 1.0,
            loss: 0.0,
            pitch: [1.0, 1.0],
            limit: None,
            range: None,
            heard_by: Hearers::Owner,
        }
    );
    assert_eq!(EffectsFile::decode(&file.encode()).unwrap(), file);
    // A refused part leaves the file as it was: a wreck that falls up, a sound of no file.
    let mut changed = file.clone();
    let up = WreckGround {
        gravity: 1.0,
        ..WreckGround::NONE
    };
    assert!(matches!(changed.set_wrecks(up), Err(EffectsError::Wrecks)));
    let mut silent = file.sounds()[&name("click")].clone();
    silent.files.clear();
    let sounds = BTreeMap::from([(name("silent"), silent)]);
    assert!(matches!(
        changed.add_sounds(sounds),
        Err(EffectsError::Sound(_))
    ));
    assert_eq!(changed, file);

    // A list's sound the file lacks, a sound of no file, a volume past 1, a pitch out of order
    // or of no rate, and a range whose silence is not past its full volume are refused.
    let refused = |from: &str, to: &str| {
        let text = text.replacen(from, to, 1);
        Toml::parse::<EffectsFile>(&text).unwrap_err().to_string()
    };
    assert!(refused("sounds = [\"tank_boom\"]", "sounds = [\"thud\"]").contains("`thud`"));
    let files = "files = [\"client/sounds/click.wav\"]";
    assert!(refused(files, "files = []").contains("`click`"));
    assert!(refused("volume = 1.0", "volume = 1.5").contains("`click`"));
    assert!(refused("[0.98, 1.02]", "[1.02, 0.98]").contains("`tank_boom`"));
    assert!(refused("[0.98, 1.02]", "[0.0, 1.02]").contains("`tank_boom`"));
    assert!(refused("silent = 800.0", "silent = 175.0").contains("`tank_boom`"));
    assert!(refused("full = 175.0", "full = -1.0").contains("`tank_boom`"));
}
