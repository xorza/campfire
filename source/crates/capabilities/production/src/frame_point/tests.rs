use campfire_codec::Toml;

use super::*;

#[test]
fn a_frame_point_turns_with_its_producer_as_its_box_does_and_reads_within_the_bound() {
    #[derive(Debug, Deserialize)]
    struct Field {
        at: FramePoint,
    }
    let point = |x: i64, z: i64| FramePoint::new([Num::int(x), Num::int(z)]).unwrap();
    let position = Vec3::new(Num::int(100), Num::int(5), Num::int(-50));
    // The frame's x is the box's first half edge, (cos, −sin), and its z the second,
    // (sin, cos): at 0° (3, 4) is 3 along x and 4 along z; at 90° x goes along −z and z
    // along x, (4, −3); at 180° both turn back, (−3, −4); at 270° (−4, 3).
    let at = |angle: i64| point(3, 4).placed(position, Num::int(angle));
    let moved = |x: i64, z: i64| Vec3::new(Num::int(100 + x), Num::int(5), Num::int(-50 + z));
    assert_eq!(at(0), moved(3, 4));
    assert_eq!(at(90), moved(4, -3));
    assert_eq!(at(180), moved(-3, -4));
    assert_eq!(at(270), moved(-4, 3));
    // A point reads from its `[x, z]`, up to the bound and no further.
    let read = |text: &str| Toml::parse::<Field>(&format!("at = {text}")).map(|field| field.at);
    assert_eq!(
        read(r#"[-18, "35.5"]"#).unwrap(),
        FramePoint([Num::int(-18), Num::int(71) / Num::int(2)])
    );
    assert_eq!(
        read("[1048576, -1048576]").unwrap(),
        point(1 << 20, -(1 << 20))
    );
    assert_eq!(
        read(r#"["1048576.5", 0]"#).unwrap_err().message(),
        "[1048576.5, 0] lies past 2²⁰ m of its producer"
    );
}
