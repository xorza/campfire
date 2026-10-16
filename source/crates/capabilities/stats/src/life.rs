use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::PerType;
use campfire_math::Num;
use campfire_sim::{SimComponent, TickRate};
use campfire_state_types::{DataKind, Replication, SentOnChange};
use campfire_units::UnitEntity;
use serde::{Deserialize, Serialize};

use crate::meter::Meter;
use crate::pool_book::PoolBook;

/// A unit's life: its meter of the mode's life pool, which every client that sees the unit
/// receives, where its other pools are `Pools`, of another kind. A unit holds it exactly when it
/// has combat; `lives` says when the unit lives.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Life(Meter);

impl Life {
    pub const fn new(meter: Meter) -> Self {
        Self(meter)
    }

    pub const fn current(self) -> Num {
        self.0.current()
    }

    pub const fn max(self) -> Num {
        self.0.max()
    }

    pub const fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    pub(crate) const fn meter(self) -> Meter {
        self.0
    }

    pub(crate) const fn meter_mut(&mut self) -> &mut Meter {
        &mut self.0
    }

    /// Takes `amount`, which is not negative, down to 0; what it took.
    pub fn take(&mut self, amount: Num) -> Num {
        self.0.take(amount)
    }

    /// Adds `amount`, which is not negative, up to its maximum.
    pub fn add(&mut self, amount: Num) {
        self.0.add(amount);
    }

    pub const fn fill(&mut self) {
        self.0.fill();
    }
}

/// Whether a unit lives: it is not `dead`, and its `life`, when it has one, is above 0. A life
/// reaches 0 in a tick before the unit's death, at the next deaths, marks it `Dead`; from then on
/// the unit lives no more, though not yet dead: nothing but its respawn lifts its life, and it
/// takes no damage or heal and starts or strikes no attack or cast. Every pass that asks whether a
/// unit lives asks this.
pub(crate) fn lives(dead: bool, life: Option<&Life>) -> bool {
    !dead && life.is_none_or(|life| !life.is_empty())
}

impl SimComponent for Life {
    const NAME: &'static str = "stats.life";
    type Host = UnitEntity;

    // A life in a match whose mode names no life pool has no pool's stats to follow; what its
    // meter carries, regen adds, as `Pools` holds.
    fn check(&self, world: &World, _: Entity) -> bool {
        let hz = world.resource::<TickRate>().hz().get();
        world
            .get_resource::<PoolBook>()
            .is_some_and(|book| book.life().is_some())
            && self.0.settled(hz)
    }
}

impl Replication for Life {
    const KIND: DataKind = DataKind::Life;
    type Sending = SentOnChange;
}

impl PerType for Life {
    const SCRIPTS_ONLY: bool = false;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_math::Num;

    use crate::life::Life;
    use crate::meter::Meter;

    impl Life {
        /// The same life with `amount` of it spent.
        #[must_use]
        pub fn spent(mut self, amount: Num) -> Self {
            self.take(amount);
            self
        }

        /// A full life of `max`, which is positive.
        pub const fn full(max: Num) -> Self {
            Self(Meter::new(max).expect("a positive maximum"))
        }
    }
}
