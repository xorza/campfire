use campfire_math::Num;
use serde::{Deserialize, Serialize};

/// A tracked or wheeled unit's chassis, which its drawing pitches and rolls on springs, as Zero
/// Hour's `Drawable::calcPhysicsXformTreads` and `calcPhysicsXformWheels` move it, each step of the
/// effects file (02-engine-core.md, A chassis follows the ground on springs): the stiffness and the
/// damping of its pitch's and its roll's springs, each a share a step, the share of each step's
/// change its axes keep, the pitch and the roll a step's change of velocity along and across its
/// facing, in metres a step, kicks it by, and the most it pitches up as it accelerates and down as
/// it brakes, in radians.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Chassis {
    pub pitch_stiffness: Num,
    pub roll_stiffness: Num,
    pub pitch_damping: Num,
    pub roll_damping: Num,
    pub axial_damping: Num,
    pub forward_pitch: Num,
    pub lateral_roll: Num,
    pub accelerating: Num,
    pub braking: Num,
}
