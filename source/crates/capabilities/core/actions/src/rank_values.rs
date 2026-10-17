use std::num::{NonZeroU8, NonZeroU16};
use std::ops::Range;

use campfire_common::{DeclaredName, Ticks};
use campfire_math::Num;
use campfire_players::ResourceAmount;
use campfire_sim::TickRate;
use campfire_stats::PoolCost;
use campfire_values::{Rank, TimeTooLarge};

use crate::action_data::ActionData;
use crate::action_range::ActionRange;
use crate::cost_target::CostTarget;
use crate::error::ActionProblem;
use crate::rank_fields::RankToggle;

/// An action's fields at each rank: its values, and its costs in player resources, each rank's
/// run in one buffer.
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
    /// Where its costs in its caster's player's resources lie in its action's buffer.
    pub(crate) resource_cost: CostRun,
    /// A weapon's own damage, which an attack of no stat of damage deals.
    pub amount: Option<Num>,
    /// A weapon's own time from one attack's start to the next, a tick at least, which an
    /// attack of no stat of rate waits.
    pub period: Option<Ticks>,
    pub clip: Option<ClipRule>,
}

/// A run of an action's buffer of resource costs: the costs of one rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CostRun {
    start: u32,
    end: u32,
}

impl CostRun {
    fn new(start: usize, end: usize) -> Self {
        let at = |place: usize| u32::try_from(place).expect("an action's costs fit u32");
        Self {
            start: at(start),
            end: at(end),
        }
    }

    /// Its places in its buffer.
    pub(crate) const fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }
}

/// A weapon's clip: the shots it fires before it reloads, and the ticks from the shot that
/// empties it to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipRule {
    pub shots: NonZeroU16,
    pub reload: Ticks,
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
            let start = loaded.resource_costs.len();
            loaded.resource_costs.extend(fields.resource_cost);
            let resource_cost = CostRun::new(start, loaded.resource_costs.len());
            loaded.values.push(Self {
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
                resource_cost,
                amount: fields.amount,
                period: fields.period_ms.map(ticks).transpose()?,
                clip: fields
                    .clip
                    .map(|clip| {
                        Ok::<_, TimeTooLarge>(ClipRule {
                            shots: clip.shots,
                            reload: ticks(clip.reload_ms)?,
                        })
                    })
                    .transpose()?,
            });
        }
        Ok(loaded)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_players::ResourceAmount;

    use crate::rank_values::CostRun;

    impl CostRun {
        /// The run of no cost.
        pub(crate) const EMPTY: Self = Self { start: 0, end: 0 };

        /// The run of the whole of `costs`, the buffer of an action of one rank.
        pub(crate) fn whole(costs: &[ResourceAmount]) -> Self {
            Self::new(0, costs.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use campfire_codec::Toml;
    use campfire_stats::PoolId;
    use campfire_values::ResourceId;

    use super::*;

    #[test]
    fn each_rank_holds_the_run_of_its_own_resource_costs() {
        // Costs in name order: gems 3 at both ranks, gold 50 then 75, and mana, a pool's, apart.
        let text = "targeting = \"none\"\ncost = { gold = [50, 75], gems = 3, mana = 10 }";
        let data = Toml::parse::<ActionData>(text).unwrap();
        let [gems, gold] = [0, 1].map(ResourceId::new);
        let target = |name: &DeclaredName| match name.as_str() {
            "gems" => Some(CostTarget::Resource(gems)),
            "gold" => Some(CostTarget::Resource(gold)),
            "mana" => PoolId::new(0).map(CostTarget::Pool),
            _ => None,
        };
        let rate = TickRate::new(NonZeroU32::new(30).unwrap());
        let loaded = RankValues::all(&data, 2, rate, target).unwrap();
        let cost = |resource, amount| ResourceAmount { resource, amount };
        let runs: Vec<&[ResourceAmount]> = (loaded.values.iter())
            .map(|values| &loaded.resource_costs[values.resource_cost.range()])
            .collect();
        assert_eq!(
            runs,
            [
                &[cost(gems, 3), cost(gold, 50)][..],
                &[cost(gems, 3), cost(gold, 75)][..],
            ]
        );
    }
}
