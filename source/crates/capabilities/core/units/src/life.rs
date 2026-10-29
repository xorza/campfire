use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::{ById, PerId};
use campfire_math::Num;
use campfire_sim::{MatchRate, SimComponent};
use campfire_states::{Replication, SentOnChange};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::meter::Meter;
use crate::unit_entity::UnitEntity;

/// A unit's life: its meter of the mode's life pool, which every client that sees the unit
/// receives, where its other pools are stats' `Pools`, of another kind. A unit holds it exactly
/// when it has combat; `lives` says when the unit lives. A contract part: the action pipeline's
/// living test reads it, and stats, which keeps the pools' maxima and regen, writes it.
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

    pub const fn meter(self) -> Meter {
        self.0
    }

    pub const fn meter_mut(&mut self) -> &mut Meter {
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
pub fn lives(dead: bool, life: Option<&Life>) -> bool {
    !dead && life.is_none_or(|life| !life.is_empty())
}

/// What a unit has of a life: none, as a delivery and a body of a type of no life have; one its
/// body takes of its type that the world does not hold, as a mode's visibility keeps an enemy's
/// life from a client; or the life it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitLife<'a> {
    Lifeless,
    Unheld,
    Held(&'a Life),
}

impl<'a> UnitLife<'a> {
    /// The life of a unit that holds `held`, or else is a `body` unit or not, of a type `typed`
    /// with a life or not: a body unit takes its type's life as it spawns, and a delivery none.
    pub const fn of(held: Option<&'a Life>, body: bool, typed: bool) -> Self {
        match held {
            Some(life) => Self::Held(life),
            None if body && typed => Self::Unheld,
            None => Self::Lifeless,
        }
    }

    /// Whether the unit, `dead` or not, has a life and lives, as `lives` says of a held one.
    pub fn lives(self, dead: bool) -> bool {
        match self {
            Self::Lifeless => false,
            Self::Unheld => !dead,
            Self::Held(life) => lives(dead, Some(life)),
        }
    }
}

impl SimComponent for Life {
    const NAME: &'static str = "units.life";
    type Host = UnitEntity;

    // A life in a match whose types have none, as a mode that names no life pool gives none, has
    // no pool's stats to follow; what its meter carries, regen adds, as `Pools` holds.
    fn check(&self, world: &World, _: Entity) -> bool {
        let hz = world.resource::<MatchRate>().hz().get();
        world
            .get_resource::<ById<UnitType, Self>>()
            .is_some_and(|lives| !lives.is_empty())
            && self.0.is_settled(hz)
    }
}

impl Replication for Life {
    const KIND: DataKind = DataKind::Life;
    type Sending = SentOnChange;
}

impl PerId<UnitType> for Life {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_has_the_life_it_holds_or_its_body_takes_of_its_type() {
        let full = Life::full(Num::int(10));
        let empty = full.spent(Num::int(10));
        // A held life counts whatever the kind and type; else a body of a type of a life has one
        // unheld, and a delivery or a type of none has none.
        assert_eq!(
            UnitLife::of(Some(&full), false, false),
            UnitLife::Held(&full)
        );
        assert_eq!(UnitLife::of(None, true, true), UnitLife::Unheld);
        assert_eq!(UnitLife::of(None, false, true), UnitLife::Lifeless);
        assert_eq!(UnitLife::of(None, true, false), UnitLife::Lifeless);
        // Alive and dead: an unheld life lives until its death, a held one while it is above 0,
        // and none never.
        let lives = |life: UnitLife<'_>| [false, true].map(|dead| life.lives(dead));
        assert_eq!(lives(UnitLife::Unheld), [true, false]);
        assert_eq!(lives(UnitLife::Held(&full)), [true, false]);
        assert_eq!(lives(UnitLife::Held(&empty)), [false, false]);
        assert_eq!(lives(UnitLife::Lifeless), [false, false]);
    }
}
