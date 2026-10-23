use std::num::NonZeroU8;
use std::ops::Range;

use campfire_common::Ticks;
use campfire_math::Num;
use campfire_players::ResourceAmount;
use campfire_units::PoolCost;
use campfire_values::Distance;

use crate::action_range::ActionRange;
use crate::rank_toggle::RankToggle;

/// An action's fields at each rank: its values, and its costs in player resources, each rank's
/// run in one buffer.
#[derive(Debug)]
pub struct LoadedRanks {
    pub(crate) values: Vec<RankValues>,
    pub(crate) resource_costs: Vec<ResourceAmount>,
}

/// The pipeline's capability fields of an action at one rank, every kind's, times in ticks; a
/// kind's own values at each rank are its capability's (04-capabilities/actions.md, Each kind owns
/// its section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RankValues {
    pub range: ActionRange,
    /// The meters nearer than which it takes no target, within its range of meters.
    pub min_range: Option<Distance>,
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
}

/// A run of an action's buffer of resource costs: the costs of one rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CostRun {
    start: u32,
    end: u32,
}

impl CostRun {
    pub(crate) fn new(start: usize, end: usize) -> Self {
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

/// How long a channel runs, none for one with no end, and the time between its ticks, in ticks,
/// each at least one; whether damage of more than 0 to its unit cuts it; the fraction its cast's
/// windup spreads each way; and the ticks an order that cuts it waits, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelRule {
    pub(crate) duration: Option<Ticks>,
    pub tick: Ticks,
    pub(crate) damage_interrupts: bool,
    pub windup_spread: Option<Num>,
    pub(crate) wind_down: Option<Ticks>,
}

/// How many charges an action holds at most, and the ticks one takes to come back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargeRule {
    pub max: NonZeroU8,
    pub recharge: Ticks,
}

impl RankValues {
    /// The meters halfway between its minimum range and its range of meters, where a unit too
    /// near its target steps back to; none without both.
    pub const fn midway(&self) -> Option<Num> {
        let (Some(min), ActionRange::Meters(range)) = (self.min_range, self.range) else {
            return None;
        };
        Some(Num::from_bits(i64::midpoint(
            min.get().to_bits(),
            range.get().to_bits(),
        )))
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
