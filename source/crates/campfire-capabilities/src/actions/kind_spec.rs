use crate::actions::action_kind::ActionKind;
use crate::actions::weapon::Weapon;
use crate::units::unit_type::UnitType;

/// What is special about an action as a match runs it, with what its kind needs: a cast; an
/// attack, with what its weapon deals; a train, with the unit type it makes; or a build, with the
/// unit type of the building it places. The load runs no other kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KindSpec {
    Cast,
    Attack(Weapon),
    Train(UnitType),
    Build(UnitType),
}

impl KindSpec {
    /// The kind as data names it.
    pub(crate) const fn kind(self) -> ActionKind {
        match self {
            KindSpec::Cast => ActionKind::Cast,
            KindSpec::Attack(_) => ActionKind::Attack,
            KindSpec::Train(_) => ActionKind::Train,
            KindSpec::Build(_) => ActionKind::Build,
        }
    }

    /// What it deals, for an attack.
    pub(crate) const fn weapon(self) -> Option<Weapon> {
        match self {
            KindSpec::Attack(weapon) => Some(weapon),
            KindSpec::Cast | KindSpec::Train(_) | KindSpec::Build(_) => None,
        }
    }
}
