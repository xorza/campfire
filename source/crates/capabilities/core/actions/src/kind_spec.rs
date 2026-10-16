use campfire_values::UnitType;

use crate::action_kind::ActionKind;
use crate::gather_spec::GatherSpec;
use crate::weapon::Weapon;

/// What is special about an action as a match runs it, with what its kind needs: a cast; an
/// attack, with what its weapon deals; a use of an object; a train, with the unit type it makes;
/// or a build, with the unit type of the building it places; or a gather, with what it gathers.
/// The load runs no other kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindSpec {
    Cast,
    Attack(Weapon),
    Use,
    Train(UnitType),
    Build(UnitType),
    Gather(GatherSpec),
}

impl KindSpec {
    /// The kind as data names it.
    pub const fn kind(self) -> ActionKind {
        match self {
            Self::Cast => ActionKind::Cast,
            Self::Attack(_) => ActionKind::Attack,
            Self::Use => ActionKind::Use,
            Self::Train(_) => ActionKind::Train,
            Self::Build(_) => ActionKind::Build,
            Self::Gather(_) => ActionKind::Gather,
        }
    }

    /// What it deals, for an attack.
    pub const fn weapon(self) -> Option<Weapon> {
        match self {
            Self::Attack(weapon) => Some(weapon),
            Self::Cast | Self::Use | Self::Train(_) | Self::Build(_) | Self::Gather(_) => None,
        }
    }
}
