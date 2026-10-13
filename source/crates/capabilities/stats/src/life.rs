use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_core::{PerType, UnitEntity};
use campfire_math::Num;
use campfire_sim::{Capability, SimComponent, TickRate};
use campfire_state_types::{DataKind, Replication, SentOnChange};
use serde::{Deserialize, Serialize};

use crate::meter::Meter;
use crate::pool_book::PoolBook;

/// A unit's life: its meter of the mode's life pool, which every client that sees the unit
/// receives, where its other pools are `Pools`, of another kind. A unit holds it exactly when it
/// has combat, and lives while it is above 0.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Life(Meter);

impl Life {
    pub const fn new(meter: Meter) -> Life {
        Life(meter)
    }

    pub const fn current(self) -> Num {
        self.0.current()
    }

    pub const fn max(self) -> Num {
        self.0.max()
    }

    pub const fn above_zero(self) -> bool {
        !self.0.is_empty()
    }

    pub(crate) const fn meter(self) -> Meter {
        self.0
    }

    pub(crate) const fn meter_mut(&mut self) -> &mut Meter {
        &mut self.0
    }

    /// Takes `amount`, which is not negative, down to 0; what it took.
    pub fn take(&mut self, amount: Num) -> Num {
        let before = self.0.current();
        self.0.take(amount);
        before - self.0.current()
    }

    /// Adds `amount`, which is not negative, up to its maximum.
    pub fn add(&mut self, amount: Num) {
        self.0.add(amount);
    }

    pub const fn fill(&mut self) {
        self.0.fill();
    }
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
    const OWNER: Option<Capability> = Some(Capability::Combat);
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
        pub fn spent(mut self, amount: Num) -> Life {
            self.take(amount);
            self
        }

        /// A full life of `max`, which is positive.
        pub const fn full(max: Num) -> Life {
            Life(Meter::new(max).expect("a positive maximum"))
        }
    }
}
