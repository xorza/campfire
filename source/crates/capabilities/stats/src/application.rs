use std::num::NonZeroU32;

use campfire_common::Ticks;
use campfire_math::Num;
use campfire_script::StateValue;
use campfire_sim::StableId;
use campfire_values::{ActionId, ModifierId, Rank};

use crate::instance::{StackEnd, StatShare};
use crate::interval::Interval;
use crate::lifetime::Lifetime;
use crate::modifier_data::Reapply;

/// A modifier applied to a unit, its numbers resolved: its instance as it would be new, with
/// how a second application from its source acts and its stack limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub instance: NewInstance,
    pub reapply: Reapply,
    pub max_stacks: Option<NonZeroU32>,
}

/// An instance as an application makes it, with its stack ends, its stat shares and its clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstance {
    pub id: ModifierId,
    pub source: Option<StableId>,
    pub action: Option<ActionId>,
    pub rank: Rank,
    pub lifetime: Lifetime,
    pub aura_radius: Option<Num>,
    pub stacks: u32,
    pub stack_life: Option<Ticks>,
    pub stack_ends: Vec<StackEnd>,
    pub interval: Option<Interval>,
    pub shield: Option<Num>,
    pub stats: Vec<StatShare>,
    pub state: Vec<StateValue>,
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_sim::StableId;
    use campfire_values::{ModifierId, Rank};

    use crate::application::{Application, NewInstance};
    use crate::lifetime::{Ends, Lifetime};
    use crate::modifier_data::Reapply;

    impl NewInstance {
        /// A new instance of `id` from `source`: one stack at rank 1 of no ability, that lasts
        /// until it is removed, with no clock, shield, stat share or state.
        pub const fn bare(id: ModifierId, source: Option<StableId>) -> Self {
            Self {
                id,
                source,
                action: None,
                rank: Rank::FIRST,
                lifetime: Lifetime::applied(Ends::Never),
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
        pub const fn refresh(instance: NewInstance) -> Self {
            Self {
                instance,
                reapply: Reapply::Refresh,
                max_stacks: None,
            }
        }
    }
}
