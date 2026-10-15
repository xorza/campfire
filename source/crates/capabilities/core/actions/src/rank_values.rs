use std::num::NonZeroU8;

use campfire_common::Ticks;
use campfire_players::ResourceAmount;
use campfire_sim::TickRate;
use campfire_stats::PoolCost;
use campfire_values::{DeclaredName, Rank, TimeTooLarge};

use crate::action_data::ActionData;
use crate::action_range::ActionRange;
use crate::cost_target::CostTarget;
use crate::error::ActionProblem;
use crate::rank_fields::RankToggle;

/// An action's fields at each rank: its values, and its cost in player resources, one run of
/// the same resources a rank.
#[derive(Debug)]
pub struct LoadedRanks {
    pub(crate) values: Vec<RankValues>,
    pub(crate) resource_costs: Vec<ResourceAmount>,
}

/// An action's capability fields at one rank, times in ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankValues {
    pub range: ActionRange,
    pub cooldown: Ticks,
    pub cost: PoolCost,
    pub windup: Ticks,
    pub charges: Option<ChargeRule>,
    pub toggle: Option<RankToggle>,
    pub channel: Option<ChannelRule>,
    /// A charged action's most, a tick at least.
    pub(crate) charge: Option<Ticks>,
}

/// How long a channel runs, and the time between its ticks, in ticks, both at least one, and
/// whether damage of more than 0 to its unit cuts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelRule {
    pub(crate) duration: Ticks,
    pub tick: Ticks,
    pub(crate) damage_interrupts: bool,
}

/// How many charges an action holds at most, and the ticks one takes to come back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargeRule {
    pub max: NonZeroU8,
    pub recharge: Ticks,
}

impl RankValues {
    /// The fields of `data` at each of its `ranks` ranks, times in ticks at `rate`, what its
    /// cost's names take from by `target`; an error for a per-rank array of another length, a
    /// field that does not hold at a rank, a use's, a build's or a gather's range that is global
    /// at a rank, or a time that does not count in ticks.
    pub(crate) fn all(
        data: &ActionData,
        ranks: u8,
        rate: TickRate,
        target: impl Fn(&DeclaredName) -> Option<CostTarget>,
    ) -> Result<LoadedRanks, ActionProblem> {
        if !data.check_ranks(usize::from(ranks)) {
            return Err(ActionProblem::RankCount(ranks));
        }
        let ticks = |ms: u64| rate.ticks(ms).ok_or(TimeTooLarge);
        let metered = data.kind.metered();
        let mut loaded = LoadedRanks {
            values: Vec::with_capacity(usize::from(ranks)),
            resource_costs: Vec::new(),
        };
        for rank in (1..=ranks).filter_map(Rank::new) {
            let fields = data
                .fields_at(rank, &target)
                .map_err(ActionProblem::Field)?;
            if let Some(kind) = metered
                && fields.range == ActionRange::Global
            {
                return Err(ActionProblem::Global(kind));
            }
            loaded.values.push(RankValues {
                range: fields.range,
                cooldown: ticks(fields.cooldown_ms)?,
                cost: fields.cost,
                windup: ticks(fields.windup_ms)?,
                charges: fields
                    .charges
                    .map(|charges| {
                        Ok::<_, TimeTooLarge>(ChargeRule {
                            max: charges.max,
                            recharge: ticks(charges.recharge_ms)?,
                        })
                    })
                    .transpose()?,
                toggle: fields.toggle,
                channel: fields
                    .channel
                    .map(|channel| {
                        Ok::<_, TimeTooLarge>(ChannelRule {
                            duration: ticks(channel.duration_ms)?,
                            tick: ticks(channel.tick_ms)?,
                            damage_interrupts: channel.damage_interrupts,
                        })
                    })
                    .transpose()?,
                charge: fields.charge_ms.map(ticks).transpose()?,
            });
            loaded.resource_costs.extend(fields.resource_cost);
        }
        Ok(loaded)
    }
}
