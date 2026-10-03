use std::num::NonZeroU8;

use campfire_common::Ticks;
use campfire_sim::TickRate;

use crate::actions::action_data::{ActionData, RankToggle};

use crate::actions::cost_target::CostTarget;

use crate::actions::error::ActionError;
use crate::actions::range::Range;
use crate::players::resource_amount::ResourceAmount;
use crate::stats::pool_cost::PoolCost;
use crate::values::declared_name::DeclaredName;

/// An action's fields at each rank: its values, and its cost in player resources, one run of
/// the same resources a rank.
#[derive(Debug)]
pub(crate) struct LoadedRanks {
    pub(crate) values: Vec<RankValues>,
    pub(crate) resource_costs: Vec<ResourceAmount>,
}

/// An action's capability fields at one rank, times in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RankValues {
    pub(crate) range: Range,
    pub(crate) cooldown: Ticks,
    pub(crate) cost: PoolCost,
    pub(crate) windup: Ticks,
    pub(crate) charges: Option<ChargeRule>,
    pub(crate) toggle: Option<RankToggle>,
}

/// How many charges an action holds at most, and the ticks one takes to come back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChargeRule {
    pub(crate) max: NonZeroU8,
    pub(crate) recharge: Ticks,
}

impl RankValues {
    /// The fields of `data`, which the package load checked, at each of its `ranks` ranks,
    /// times in ticks at `rate`, what its cost's names take from by `target`; an error when a
    /// time does not count in ticks.
    pub(crate) fn all(
        data: &ActionData,
        ranks: u8,
        rate: TickRate,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<LoadedRanks, ActionError> {
        assert!(
            data.check_ranks(usize::from(ranks)),
            "the load checked the ranks"
        );
        let ticks = |ms: u64| rate.ticks(ms).ok_or(ActionError::TimeTooLarge);
        let mut loaded = LoadedRanks {
            values: Vec::with_capacity(usize::from(ranks)),
            resource_costs: Vec::new(),
        };
        for rank in 1..=ranks {
            let fields = data
                .fields_at(rank, &target)
                .expect("the load checked the fields");
            loaded.values.push(RankValues {
                range: fields.range,
                cooldown: ticks(fields.cooldown_ms)?,
                cost: fields.cost,
                windup: ticks(fields.windup_ms)?,
                charges: fields
                    .charges
                    .map(|charges| {
                        Ok(ChargeRule {
                            max: charges.max,
                            recharge: ticks(charges.recharge_ms)?,
                        })
                    })
                    .transpose()?,
                toggle: fields.toggle,
            });
            loaded.resource_costs.extend(fields.resource_cost);
        }
        Ok(loaded)
    }
}
