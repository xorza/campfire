use campfire_math::Num;
use campfire_units::Facing;
use serde::{Deserialize, Serialize};

/// How a unit moves through a crowd (04-capabilities/navigation.md, A unit moves by its kind of
/// motion): it steps, as infantry, and collision parts it from another any way, or it drives, as a
/// vehicle, and collision moves it only along its facing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gait {
    Step,
    Drive,
}

impl Gait {
    /// Whether a walker of this gait may move a unit of `other` aside on a way no unit of its team
    /// blocks: one that steps moves no unit that drives, as Zero Hour's `moveAllies` lets no
    /// infantry push a vehicle off a path no ally blocks (entries-and-blocking.md, A walker that
    /// steps moves no unit that drives aside).
    pub(crate) const fn moves(self, other: Self) -> bool {
        !matches!((self, other), (Self::Step, Self::Drive))
    }
}

/// The facing of a unit that steps in a mode whose steppers pass, as the brake and collision read
/// it, none for one that drives or of a mode whose steppers block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stepping(pub(crate) Option<Facing>);

impl Stepping {
    /// The stepping of a unit that passes none.
    pub(crate) const NONE: Self = Self(None);

    /// Whether it and `other` are of two units that step and face no one way: the dot of their
    /// facings is a quarter or less, as Zero Hour's `blockedBy` lets infantry that do not move in
    /// the same direction pass through each other (entries-and-blocking.md, Walkers that step pass
    /// those that step another way).
    pub(crate) fn crosses(self, other: Self) -> bool {
        let (Some(a), Some(b)) = (self.0, other.0) else {
            return false;
        };
        let [a_x, a_z] = a.way();
        let [b_x, b_z] = b.way();
        a_x * b_x + a_z * b_z <= Num::QUARTER
    }
}
