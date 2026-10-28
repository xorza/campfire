use campfire_math::{Num, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

/// A Zero Hour locomotor, its speed, turn rate and acceleration its unit's stats (driving.md, Each
/// appearance moves as its mover): its appearance; how fast it brakes, in meters a second squared,
/// above 0; the speed in meters a second a leg slows to as it comes to its goal, its `MinSpeed`;
/// the speed a wheel holds in a turn, at least, its `MinTurnSpeed`; the meters ahead of its center
/// it turns about, its `TurnPivotOffset` times its bounding radius (driving.md, A turn moves the
/// body round its pivot); whether a wheel backs toward a goal behind it, its `CanMoveBackwards`;
/// the meters past which its goal along its route makes it turn in three points, five major
/// radii; the share of its sideways velocity it loses each tick, its `LateralFriction`, 0.15 with
/// none; the meters from its route's end within which it has arrived, its `CloseEnoughDist`, 1
/// with none; and whether, in a cell it may not stand in, it pushes itself out, as
/// `fixInvalidPosition` does for every mover but a dozer, true with none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ZeroHourLocomotor {
    pub appearance: LocomotorAppearance,
    pub braking: Num,
    pub min_speed: Num,
    pub min_turn_speed: Num,
    pub pivot: Num,
    pub backwards: bool,
    pub three_point: Num,
    pub lateral_friction: Num,
    pub close_enough: Num,
    pub corrects: bool,
}

/// How a Zero Hour locomotor drives, as its `Appearance` names it: on treads, on two legs, on four
/// wheels or on a motorcycle's two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocomotorAppearance {
    Treads,
    TwoLegs,
    FourWheels,
    Motorcycle,
}

impl ZeroHourLocomotor {
    /// The lateral friction of a locomotor that gives none, `DEFAULT_LATERAL_FRICTION`.
    pub const LATERAL_FRICTION: Num = Num::from_bits(2_516_582);

    /// The locomotor of `appearance` that brakes at `braking` and otherwise moves as Zero Hour's
    /// default does: no least speed, no turning speed, no pivot, no backing, the default lateral
    /// friction, a meter's close enough distance, and a push out of a cell it may not stand in.
    pub const fn of(appearance: LocomotorAppearance, braking: Num) -> Self {
        Self {
            appearance,
            braking,
            min_speed: Num::ZERO,
            min_turn_speed: Num::ZERO,
            pivot: Num::ZERO,
            backwards: false,
            three_point: Num::ZERO,
            lateral_friction: Self::LATERAL_FRICTION,
            close_enough: Num::ONE,
            corrects: true,
        }
    }

    /// Whether its numbers hold: a braking above 0, speeds and distances at least 0, each within
    /// the world's bound, a pivot within it either way, and a friction from 0 to 1.
    pub fn holds(&self) -> bool {
        let bounded = |value: Num| (Num::ZERO..=Vec3::BOUND).contains(&value);
        self.braking > Num::ZERO
            && bounded(self.braking)
            && bounded(self.min_speed)
            && bounded(self.min_turn_speed)
            && bounded(self.three_point)
            && bounded(self.close_enough)
            && (-Vec3::BOUND..=Vec3::BOUND).contains(&self.pivot)
            && (Num::ZERO..=Num::ONE).contains(&self.lateral_friction)
    }
}

/// Data is untrusted, so a locomotor of numbers that do not hold fails to read; a field it leaves
/// out is `of`'s.
impl<'de> Deserialize<'de> for ZeroHourLocomotor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            appearance: LocomotorAppearance,
            braking: Num,
            #[serde(default)]
            min_speed: Num,
            #[serde(default)]
            min_turn_speed: Num,
            #[serde(default)]
            pivot: Num,
            #[serde(default)]
            backwards: bool,
            #[serde(default)]
            three_point: Num,
            lateral_friction: Option<Num>,
            close_enough: Option<Num>,
            corrects: Option<bool>,
        }
        let fields = Fields::deserialize(deserializer)?;
        let defaults = Self::of(fields.appearance, fields.braking);
        let locomotor = Self {
            min_speed: fields.min_speed,
            min_turn_speed: fields.min_turn_speed,
            pivot: fields.pivot,
            backwards: fields.backwards,
            three_point: fields.three_point,
            lateral_friction: fields.lateral_friction.unwrap_or(defaults.lateral_friction),
            close_enough: fields.close_enough.unwrap_or(defaults.close_enough),
            corrects: fields.corrects.unwrap_or(defaults.corrects),
            ..defaults
        };
        locomotor.holds().then_some(locomotor).ok_or_else(|| {
            D::Error::custom(
                "a locomotor brakes above 0, its speeds and distances lie from 0 to the world's \
                 bound, and its friction from 0 to 1",
            )
        })
    }
}
