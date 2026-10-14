use std::num::NonZeroU32;

use campfire_values::{DeclaredName, Scalar, Stat};

use crate::action_data::ActionData;
use crate::action_data_field::ActionDataField;
use crate::action_kind::ActionKind;

/// What an action of a kind the release runs needs, as its data gives it: a cast and a use
/// nothing; an attack its weapon's stats of attacks a second and of damage, and its damage kind; a
/// train and a build their unit type; a gather its resource, the most a trip carries, and its bounce, none
/// when absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KindData<'a> {
    Cast,
    Attack {
        rate: &'a Stat,
        damage: &'a Stat,
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
        bounce: Option<Scalar>,
    },
}

impl<'a> KindData<'a> {
    /// What the kind of `data`, one the release runs, needs, as `data` gives it; an error names the
    /// first field the kind refuses, or needs and `data` does not give, as the table of action
    /// fields says.
    pub fn of(data: &'a ActionData) -> Result<KindData<'a>, ActionDataField> {
        if let Some(field) = ActionDataField::misused(data, data.kind) {
            return Err(field);
        }
        let needs = "the table of action fields makes its kind need it";
        Ok(match data.kind {
            ActionKind::Cast => KindData::Cast,
            ActionKind::Attack => KindData::Attack {
                rate: data.rate.as_ref().expect(needs),
                damage: data.damage.as_ref().expect(needs),
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
            kind => panic!("the release runs no {kind:?}"),
        })
    }
}
