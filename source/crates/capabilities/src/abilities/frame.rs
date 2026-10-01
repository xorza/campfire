use std::collections::BTreeMap;

use campfire_math::Num;
use campfire_sim::StableId;

use crate::abilities::ability_book::AbilityId;
use crate::combat::damage_kind::DamageKind;
use crate::scripts::error::CallError;
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::modifier_handle::ModifierHandle;
use crate::values::name_table::NameTable;
use crate::values::param::Param;
use crate::values::scalar::Scalar;

/// What `on_cast` calls read and queue, beside the units the view holds. One frame serves the
/// whole match, its buffers cleared and filled again, so a cast allocates none of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// Every loaded ability's params, one run per ability, by ability id.
    params: NameTable<Param>,
    /// The running cast's ability, its rank, and its caster.
    cast: Option<AbilityId>,
    pub(crate) rank: u8,
    pub(crate) caster: Option<StableId>,
    /// The running cast's params at its rank, in the order of their names.
    values: Vec<Scalar>,
    /// The effects the running cast queued, in order, and the modifier handles it took.
    pub(crate) effects: Vec<Effect>,
    pub(crate) handles: Vec<ModifierHandle>,
}

/// An effect a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Damage {
        target: StableId,
        amount: Num,
        kind: DamageKind,
    },
    Heal {
        unit: StableId,
        amount: Num,
    },
    Restore {
        unit: StableId,
        amount: Num,
    },
    Modifier(ModifierEffect),
}

impl Frame {
    /// Adds the params of `ability`, the one the book loads next.
    pub(crate) fn add_params(&mut self, ability: AbilityId, params: &BTreeMap<String, Param>) {
        let run = self.params.push(
            params
                .iter()
                .map(|(name, param)| (name.as_str(), param.clone())),
        );
        debug_assert_eq!(run, ability.index(), "one run of params per ability");
    }

    /// Starts a cast of `ability` at `rank`, with its params at that rank; a param that
    /// overflows there fails the cast.
    pub(crate) fn begin_cast(
        &mut self,
        ability: AbilityId,
        rank: u8,
        caster: StableId,
    ) -> Result<(), CallError> {
        self.cast = Some(ability);
        self.rank = rank;
        self.caster = Some(caster);
        self.effects.clear();
        self.handles.clear();
        let params = self.params.values(ability.index());
        self.values.clear();
        self.values.reserve_exact(params.len());
        for param in params {
            self.values
                .push(param.at(rank).ok_or(CallError::ParamOverflow)?);
        }
        Ok(())
    }

    /// Param `name` of `ability` at `rank`, if it declares one and it resolves there.
    pub(crate) fn ability_param(&self, ability: AbilityId, rank: u8, name: &str) -> Option<Scalar> {
        let at = self.params.find(ability.index(), name)?;
        self.params.values(ability.index())[at].at(rank)
    }

    pub(crate) const fn cast(&self) -> Option<AbilityId> {
        self.cast
    }

    /// The running cast's param `name`, if its ability declares one.
    pub(crate) fn param(&self, name: &str) -> Option<Scalar> {
        let at = self.params.find(self.cast?.index(), name)?;
        Some(self.values[at])
    }
}
