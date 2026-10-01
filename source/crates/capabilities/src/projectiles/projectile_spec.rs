use campfire_math::Num;

use crate::units::filter::Filter;

/// A projectile type as a match runs it: its speed a tick, its width, its range if it has its
/// own, whether it homes, stops at its first hit, or hits a unit once a cast, and what it hits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectileSpec {
    pub(crate) speed: Num,
    pub(crate) width: Num,
    pub(crate) range: Option<Num>,
    pub(crate) homing: bool,
    pub(crate) stop_on_hit: bool,
    pub(crate) once_per_cast: bool,
    pub(crate) hits: Filter,
}
