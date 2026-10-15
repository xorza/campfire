use campfire_math::{Num, SinCos, Vec3};
use campfire_sim::Position;
use serde::Deserialize;

use crate::error::PointPastBound;

/// A point in a producer's own frame, `[x, z]` in meters from its position, each within
/// `Position::BOUND`: where its trained units come out, or go with no rally point set. It turns
/// with the producer as its box body turns by its angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "[Num; 2]")]
pub struct FramePoint([Num; 2]);

impl FramePoint {
    /// `[x, z]`; `None` for a coordinate past `Position::BOUND`.
    pub fn new(point: [Num; 2]) -> Option<FramePoint> {
        let within =
            |at: &Num| at.to_bits().unsigned_abs() <= Position::BOUND.to_bits().unsigned_abs();
        point.iter().all(within).then_some(FramePoint(point))
    }

    /// The point of a producer at `pos` that faces `angle` degrees, at its height: the frame's
    /// `x` turned as its box's first half edge turns and its `z` as its second, from `x`
    /// towards `−z`, each coordinate rounded once.
    pub fn placed(self, pos: Vec3, angle: Num) -> Vec3 {
        let [x, z] = self.0;
        let turned = Vec3::new(x, Num::ZERO, z).rotated_y(SinCos::of_degrees(angle));
        Vec3::new(pos.x + turned.x, pos.y, pos.z + turned.z)
    }
}

impl TryFrom<[Num; 2]> for FramePoint {
    type Error = PointPastBound;

    fn try_from(point: [Num; 2]) -> Result<FramePoint, PointPastBound> {
        FramePoint::new(point).ok_or(PointPastBound(point))
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_frame_point_turns_with_its_producer_as_its_box_does_and_reads_within_the_bound() {
        #[derive(Debug, Deserialize)]
        struct Field {
            at: FramePoint,
        }
        let point = |x: i64, z: i64| FramePoint::new([Num::int(x), Num::int(z)]).unwrap();
        let pos = Vec3::new(Num::int(100), Num::int(5), Num::int(-50));
        // The frame's x is the box's first half edge, (cos, −sin), and its z the second,
        // (sin, cos): at 0° (3, 4) is 3 along x and 4 along z; at 90° x goes along −z and z
        // along x, (4, −3); at 180° both turn back, (−3, −4); at 270° (−4, 3).
        let at = |angle: i64| point(3, 4).placed(pos, Num::int(angle));
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
        assert!(read(r#"["1048576.5", 0]"#).is_err());
    }
}
