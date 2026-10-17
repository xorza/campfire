use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_values::{Distance, Stat};

use crate::action_data::ActionData;
use crate::action_data_field::ActionDataField;
use crate::action_kind::ActionKind;
use crate::error::KindError;

/// What an action of a kind the release runs needs, as its data gives it: a cast and a use
/// nothing; an attack its weapon's rate and damage, each a stat or its own, and its damage kind;
/// a train and a build their unit type; a gather its resource, the most a trip carries, and its
/// bounce, none when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindData<'a> {
    Cast,
    Attack {
        rate: FromData<'a>,
        damage: FromData<'a>,
        damage_kind: &'a DeclaredName,
    },
    Use,
    Train {
        unit_type: &'a DeclaredName,
    },
    Build {
        unit_type: &'a DeclaredName,
    },
    Gather {
        resource: &'a DeclaredName,
        take: NonZeroU32,
        bounce: Option<Distance>,
    },
}

/// Where a weapon's value comes from, as its data gives it: a stat of its attacker, or a field of
/// its own at each rank, `period_ms` for its rate and `amount` for its damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FromData<'a> {
    Stat(&'a Stat),
    Own,
}

impl<'a> KindData<'a> {
    /// What the kind of `data` needs, as `data` gives it; an error for a kind the release does not
    /// run, and one that names the first field the kind refuses, or needs and `data` does not
    /// give, as the table of action fields says.
    pub fn of(data: &'a ActionData) -> Result<Self, KindError> {
        if !data.kind.runs() {
            return Err(KindError::NotRun(data.kind));
        }
        if let Some(field) = ActionDataField::misused(data, data.kind) {
            return Err(KindError::Field(field));
        }
        let needs = "the table of action fields makes its kind need it";
        Ok(match data.kind {
            ActionKind::Cast => KindData::Cast,
            ActionKind::Attack => KindData::Attack {
                rate: data.rate.as_ref().map_or(FromData::Own, FromData::Stat),
                damage: data.damage.as_ref().map_or(FromData::Own, FromData::Stat),
                damage_kind: data.damage_kind.as_ref().expect(needs),
            },
            ActionKind::Use => KindData::Use,
            ActionKind::Train => KindData::Train {
                unit_type: data.unit_type.as_ref().expect(needs),
            },
            ActionKind::Build => KindData::Build {
                unit_type: data.unit_type.as_ref().expect(needs),
            },
            ActionKind::Gather => KindData::Gather {
                resource: data.resource.as_ref().expect(needs),
                take: data.take.expect(needs),
                bounce: data.bounce,
            },
            ActionKind::Enter | ActionKind::Craft => {
                unreachable!("the release runs no {:?}", data.kind)
            }
        })
    }
}
