use std::num::NonZeroU32;

use campfire_math::{Num, Ticks};
use campfire_sim::StableId;

use crate::scripts::state_value::StateValue;
use crate::stats::instance::StackEnd;
use crate::stats::instance::StatShare;
use crate::stats::lifetime::Lifetime;
use crate::stats::modifier_clocks::Interval;
use crate::stats::modifier_data::Reapply;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;

/// A modifier applied to a unit, its numbers resolved: its instance as it would be new, with
/// how a second application from its source acts and its stack limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Application {
    pub(crate) instance: NewInstance,
    pub(crate) reapply: Reapply,
    pub(crate) max_stacks: Option<NonZeroU32>,
}

/// An instance as an application makes it, with its stack ends, its stat shares and its clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewInstance {
    pub(crate) id: ModifierId,
    pub(crate) source: Option<StableId>,
    pub(crate) ability: Option<ActionId>,
    pub(crate) rank: u8,
    pub(crate) lifetime: Lifetime,
    pub(crate) aura_radius: Option<Num>,
    pub(crate) stacks: u32,
    pub(crate) stack_life: Option<Ticks>,
    pub(crate) stack_ends: Vec<StackEnd>,
    pub(crate) interval: Option<Interval>,
    pub(crate) shield: Option<Num>,
    pub(crate) stats: Vec<StatShare>,
    pub(crate) state: Vec<StateValue>,
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_sim::StableId;

    use crate::stats::application::{Application, NewInstance};
    use crate::stats::lifetime::{Ends, Lifetime};
    use crate::stats::modifier_data::Reapply;
    use crate::units::modifier_id::ModifierId;

    impl NewInstance {
        /// A new instance of `id` from `source`: one stack at rank 1 of no ability, that lasts
        /// until it is removed, with no clock, shield, stat share or state.
        pub(crate) fn bare(id: ModifierId, source: Option<StableId>) -> NewInstance {
            NewInstance {
                id,
                source,
                ability: None,
                rank: 1,
                lifetime: Lifetime::new(None, Ends::Never),
                aura_radius: None,
                stacks: 1,
                stack_life: None,
                stack_ends: Vec::new(),
                interval: None,
                shield: None,
                stats: Vec::new(),
                state: Vec::new(),
            }
        }
    }

    impl Application {
        /// `instance`, which a second application refreshes, with no stack limit.
        pub(crate) const fn refresh(instance: NewInstance) -> Application {
            Application {
                instance,
                reapply: Reapply::Refresh,
                max_stacks: None,
            }
        }
    }
}
