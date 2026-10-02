use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::stats::lifetime::Lifetime;
use crate::stats::live_param::LiveParam;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;

/// A modifier a unit carries, its numbers resolved when it was applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Instance {
    pub(crate) id: ModifierId,
    /// The unit that applied it; none from the mode.
    pub(crate) source: Option<StableId>,
    /// The ability whose cast, projectile, area or modifier applied it, or whose passive it is,
    /// at its rank on the source.
    pub(crate) ability: Option<ActionId>,
    pub(crate) rank: u8,
    /// What keeps it: an ability's passive, which a death keeps, an aura, an area or its
    /// carrier's player, and an application of its own.
    pub(crate) lifetime: Lifetime,
    /// The radius of the aura it gives, when its modifier has one.
    pub(crate) aura_radius: Option<Num>,
    pub(crate) stacks: u32,
    /// How long each stack holds, when its stacks end one by one.
    pub(crate) stack_life: Option<Ticks>,
    /// How many stack ends, and stat shares, it has in its carrier's buffers.
    pub(super) ends: u16,
    pub(super) shares: u16,
}

/// The stacks of an instance that end as tick `until` starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StackEnd {
    pub(crate) until: Tick,
    pub(crate) count: u32,
}

/// The value a stack of a modifier's change of one stat adds: `value`, which the refresh reads
/// again from `live` when the change reads a scaling table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct StatShare {
    pub(crate) value: Num,
    pub(crate) live: Option<LiveParam>,
}

impl Instance {
    /// The first tick a modifier or stack of `ticks` applied in tick `now` no longer holds: it
    /// holds through tick `now + ticks`.
    pub(crate) const fn end(now: Tick, ticks: Ticks) -> Tick {
        now.after(ticks).after(Ticks::ONE)
    }
}
