use campfire_codec::Toml;
use campfire_math::Num;

use crate::locomotor_data::LocomotorData;
use crate::zero_hour::zero_hour_locomotor::{LocomotorAppearance, ZeroHourLocomotor};

#[test]
fn a_locomotor_reads_its_fields_its_defaults_and_refuses_numbers_that_do_not_hold() {
    // A Humvee's: wheels braking at 1,000, turning at 20 at least about 0.33 of its 15 m
    // radius behind its center, backing, three-point past 75 m.
    let humvee: LocomotorData = Toml::parse(
        r#"zero_hour = { appearance = "four_wheels", braking = "1000", min_turn_speed = "20",
           pivot = "-4.95", backwards = true, three_point = "75" }"#,
    )
    .unwrap();
    assert_eq!(
        humvee,
        LocomotorData::ZeroHour(ZeroHourLocomotor {
            min_turn_speed: Num::int(20),
            pivot: "-4.95".parse().unwrap(),
            backwards: true,
            three_point: Num::int(75),
            ..ZeroHourLocomotor::of(LocomotorAppearance::FourWheels, Num::int(1000))
        })
    );
    // The default friction is 0.15, to the bit a `Num` holds nearest.
    let fifteen: Num = "0.15".parse().unwrap();
    assert_eq!(ZeroHourLocomotor::LATERAL_FRICTION, fifteen);
    let close: LocomotorData = Toml::parse(
        r#"zero_hour = { appearance = "treads", braking = "1", close_enough = "4",
           lateral_friction = "0.5" }"#,
    )
    .unwrap();
    assert_eq!(
        close,
        LocomotorData::ZeroHour(ZeroHourLocomotor {
            close_enough: Num::int(4),
            lateral_friction: Num::HALF,
            ..ZeroHourLocomotor::of(LocomotorAppearance::Treads, Num::ONE)
        })
    );
    for refused in [
        r#"zero_hour = { appearance = "treads", braking = "0" }"#,
        r#"zero_hour = { appearance = "treads", braking = "1", min_speed = "-1" }"#,
        r#"zero_hour = { appearance = "treads", braking = "1", lateral_friction = "1.5" }"#,
        r#"zero_hour = { appearance = "treads", braking = "1", close_enough = "-1" }"#,
        r#"zero_hour = { appearance = "hover", braking = "1" }"#,
    ] {
        assert!(Toml::parse::<LocomotorData>(refused).is_err(), "{refused}");
    }
}
