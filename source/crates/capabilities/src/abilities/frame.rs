use std::collections::BTreeMap;

use campfire_math::Num;
use campfire_sim::StableId;

use crate::abilities::ability_book::AbilityId;
use crate::scripts::error::CallError;
use crate::values::name_table::NameTable;
use crate::values::param::Param;
use crate::values::scalar::Scalar;

/// What `on_cast` calls read and queue, beside the units the view holds. One frame serves the
/// whole match, its buffers cleared and filled again, so a cast allocates none of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// Every loaded ability's params, one run per ability, by ability id.
    params: NameTable<Param>,
    /// The running cast's ability.
    cast: Option<AbilityId>,
    /// The running cast's params at its rank, in the order of their names.
    values: Vec<Scalar>,
    /// The effects the running cast queued, in order.
    pub(crate) effects: Vec<Effect>,
}

/// An effect a call queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Damage { target: StableId, amount: Num },
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
    pub(crate) fn begin_cast(&mut self, ability: AbilityId, rank: u8) -> Result<(), CallError> {
        self.cast = Some(ability);
        self.effects.clear();
        let params = self.params.values(ability.index());
        self.values.clear();
        self.values.reserve_exact(params.len());
        for param in params {
            self.values
                .push(param.at(rank).ok_or(CallError::ParamOverflow)?);
        }
        Ok(())
    }

    /// The running cast's param `name`, if its ability declares one.
    pub(crate) fn param(&self, name: &str) -> Option<Scalar> {
        let at = self.params.find(self.cast?.index(), name)?;
        Some(self.values[at])
    }
}
