use campfire_codec::Toml;

use super::*;
use crate::spread::Spread;

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
}
