use campfire_units::Facing;
use serde::Deserialize;

use crate::gait::{Gait, Stepping};

/// Whether walkers that step pass those that step another way, as a mode's navigation names it
/// (entries-and-blocking.md, Walkers that step pass those that step another way): never, as Dota
/// 2's units block one another, or while the two face no one way, as Zero Hour's infantry walks
/// through infantry it crosses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Crossing {
    #[default]
    Block,
    StepsPass,
}

impl Crossing {
    /// The stepping the brake and collision read of a unit of `gait` that faces `facing`: its
    /// facing where it steps and steppers pass, none else.
    pub(crate) fn stepping(self, gait: Gait, facing: Facing) -> Stepping {
        Stepping((self == Self::StepsPass && gait == Gait::Step).then_some(facing))
    }
}
