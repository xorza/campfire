use std::num::NonZeroU32;

use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;
use crate::values::stat::Stat;

/// What an action of a kind the release runs needs, as its data gives it: a cast nothing; an
/// attack its weapon's stats of attacks a second and of damage, and its damage kind; a train and
/// a build their unit type; a gather its resource, the most a trip carries, and its bounce, none
/// when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindData<'a> {
    Cast,
    Attack {
        rate: &'a Stat,
        damage: &'a Stat,
        damage_kind: &'a DeclaredName,
    },
    Train {
        unit_type: &'a DeclaredName,
    },
    Build {
        unit_type: &'a DeclaredName,
    },
    Gather {
        resource: &'a DeclaredName,
        take: NonZeroU32,
        bounce: Option<Scalar>,
    },
}
