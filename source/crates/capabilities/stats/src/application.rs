use std::num::NonZeroU32;

use campfire_common::{Tick, Ticks};
use campfire_math::Num;
use campfire_script::StateValue;
use campfire_sim::{MatchRate, StableId};
use campfire_values::{ActionId, ModifierId, Rank};

use crate::applier::Applier;
use crate::instance::{StackEnd, StatShare};
use crate::interval::Interval;
use crate::lifetime::Lifetime;
use crate::modifier_data::Reapply;
use crate::param_book::ParamBook;
use crate::param_source::ParamSource;

/// A modifier applied to a unit, its numbers resolved: its instance as it would be new, with
/// how a second application from its source acts and its stack limit. Its stat shares `S` and
/// its state `T` are what its carrier's buffers take, as the book reads them, so an application
/// allocates nothing of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application<S = Vec<StatShare>, T = Vec<StateValue>> {
    pub instance: NewInstance<S, T>,
    pub reapply: Reapply,
    pub max_stacks: Option<NonZeroU32>,
}

/// Where and when a modifier applies: the tick, the rate its times count in, and the param book
/// its numbers read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ApplyAt<'a> {
    pub(crate) now: Tick,
    pub(crate) tick_rate: MatchRate,
    pub(crate) params: &'a ParamBook,
}

/// A modifier an applier adds to a unit: its id, who applies it, the applier's source as its
/// params read it, none when it is gone or there is none, and the duration a call names.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Adding<'a> {
    pub(crate) id: ModifierId,
    pub(crate) applier: Applier,
    pub(crate) source: Option<&'a ParamSource<'a>>,
    pub(crate) duration: Option<Ticks>,
}

/// An instance as an application makes it, with the end of its stack, its stat shares and its
/// clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstance<S = Vec<StatShare>, T = Vec<StateValue>> {
    pub id: ModifierId,
    pub source: Option<StableId>,
    pub action: Option<ActionId>,
    pub rank: Rank,
    pub lifetime: Lifetime,
    pub aura_radius: Option<Num>,
    pub stacks: u32,
    pub stack_life: Option<Ticks>,
    /// With a stack life, when its one stack ends.
    pub stack_end: Option<StackEnd>,
    pub interval: Option<Interval>,
    pub shield: Option<Num>,
    pub stats: S,
    pub state: T,
}

impl<S, T> Application<S, T>
where
    S: IntoIterator<Item = StatShare>,
    T: IntoIterator<Item = StateValue>,
{
    /// The same application, its shares and its state held: for a carrier read after its
    /// sources are, as a spawn's hook reads one.
    pub fn owned(self) -> Application {
        let Self {
            instance,
            reapply,
            max_stacks,
        } = self;
        let NewInstance {
            id,
            source,
            action,
            rank,
            lifetime,
            aura_radius,
            stacks,
            stack_life,
            stack_end,
            interval,
            shield,
            stats,
            state,
        } = instance;
        Application {
            instance: NewInstance {
                id,
                source,
                action,
                rank,
                lifetime,
                aura_radius,
                stacks,
                stack_life,
                stack_end,
                interval,
                shield,
                stats: stats.into_iter().collect(),
                state: state.into_iter().collect(),
            },
            reapply,
            max_stacks,
        }
    }
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
                stack_end: None,
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
