use std::fmt;
use std::marker::PhantomData;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_sim::{DueIndex, SimComponent, SimTick, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{NotSent, Replication};
use campfire_units::{Dead, UnitEntity};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

/// What a unit takes a turn of each period of its type's, as its book `ById<UnitType, Self>` holds the
/// period: a scan or a think.
pub trait TurnSpec: Send + Sync + 'static {
    /// The name of the part of its turns in a snapshot.
    const NAME: &'static str;

    fn period(&self) -> Ticks;
}

/// The tick a unit is due in again for its turn of `S`, a part of each unit of a type whose book
/// gives `S`: none before its first turn, which comes in the first tick that leaves the
/// remainder of its stable id when divided by its type's period, so the units of a period spread
/// over it (04-capabilities/control.md, AI thinks in staggered turns), then a period after each.
/// Its hook pushes its unit into the index of its turns at each insert, a spawn's and a
/// restore's alike.
#[derive(Component, Serialize, Deserialize)]
#[component(immutable, on_insert = Self::pushed)]
#[serde(transparent, bound = "")]
pub struct NextTurn<S: TurnSpec> {
    next: Option<Tick>,
    #[serde(skip)]
    spec: PhantomData<fn() -> S>,
}

impl<S: TurnSpec> NextTurn<S> {
    /// Before the unit's first turn.
    pub const FIRST: Self = Self {
        next: None,
        spec: PhantomData,
    };

    pub const fn new(tick: Tick) -> Self {
        Self {
            next: Some(tick),
            spec: PhantomData,
        }
    }

    pub const fn get(self) -> Option<Tick> {
        self.next
    }

    /// The tick its unit, `id`, of a type of a turn each `period`, is due in, as the tick `now`
    /// starts: its next, or before its first, the first tick from `now` that leaves the
    /// remainder of its stable id when divided by the period.
    pub(crate) const fn due_at(self, id: StableId, period: Ticks, now: Tick) -> Tick {
        if let Some(next) = self.next {
            return next;
        }
        let period = period.get();
        let wait = (id.get() % period + period - now.get() % period) % period;
        Tick::new(now.get() + wait)
    }

    /// Pushes its unit into the index of its turns, at its due tick, in a match whose book gives
    /// its type a turn.
    fn pushed(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let unit = world.entity(entity);
        let parts = (
            unit.get::<Self>(),
            unit.get::<StableId>(),
            unit.get::<UnitType>(),
            unit.contains::<Dead>(),
        );
        let (Some(&next), Some(&id), Some(&unit_type), false) = parts else {
            return;
        };
        let book = world.get_resource::<ById<UnitType, S>>();
        let Some(spec) = book.and_then(|book| book.get(unit_type)) else {
            return;
        };
        let Some(now) = world.get_resource::<SimTick>().map(|tick| tick.start()) else {
            return;
        };
        let at = next.due_at(id, spec.period(), now);
        if let Some(mut index) = world.get_resource_mut::<DueIndex<Self>>() {
            index.push(at, id, entity);
        }
    }

    /// Pushes the unit of `revived`, which lives again, into the index of its turns, which
    /// waited while it was dead.
    pub(crate) fn revived(
        revived: On<'_, '_, Remove<Dead>>,
        units: Query<'_, '_, (&StableId, &UnitType, &Self)>,
        (book, tick): (Option<Res<'_, ById<UnitType, S>>>, Res<'_, SimTick>),
        index: Option<ResMut<'_, DueIndex<Self>>>,
    ) {
        let entity = revived.entity;
        let (Ok((&id, &unit_type, &next)), Some(mut index)) = (units.get(entity), index) else {
            return;
        };
        if let Some(spec) = book.as_ref().and_then(|book| book.get(unit_type)) {
            index.push(next.due_at(id, spec.period(), tick.start()), id, entity);
        }
    }
}

impl<S: TurnSpec> fmt::Debug for NextTurn<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("NextTurn").field(&self.next).finish()
    }
}

impl<S: TurnSpec> Clone for NextTurn<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: TurnSpec> Copy for NextTurn<S> {}

impl<S: TurnSpec> PartialEq for NextTurn<S> {
    fn eq(&self, other: &Self) -> bool {
        self.next == other.next
    }
}

impl<S: TurnSpec> Eq for NextTurn<S> {}

/// A living unit of a type whose book gives it a turn is due at its next; a dead one's waits
/// (04-capabilities/00-overview.md, One due index for each timed process).
impl<S: TurnSpec> TimedProcess for NextTurn<S> {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (
        &'static Self,
        &'static StableId,
        &'static UnitType,
        Has<Dead>,
    );
    type Books = (
        Option<Res<'static, ById<UnitType, S>>>,
        Res<'static, SimTick>,
    );

    fn due(
        (book, tick): &(Option<Res<'_, ById<UnitType, S>>>, Res<'_, SimTick>),
        (next, &id, &unit_type, dead): (&Self, &StableId, &UnitType, bool),
    ) -> Option<Tick> {
        let spec = book.as_ref()?.get(unit_type).filter(|_| !dead)?;
        Some(next.due_at(id, spec.period(), tick.start()))
    }
}

impl<S: TurnSpec> SimComponent for NextTurn<S> {
    const NAME: &'static str = S::NAME;
    type Host = UnitEntity;

    // A turn sets the next a period on, so a unit of a type with a turn is due at most a period
    // after the tick the match runs next; a later tick would keep it from its turn.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let Some(next) = self.next else {
            return true;
        };
        let now = world.resource::<SimTick>().start();
        let spec = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| world.resource::<ById<UnitType, S>>().get(unit_type));
        spec.is_some_and(|spec| next <= now.after(spec.period()))
    }
}

impl<S: TurnSpec> Replication for NextTurn<S> {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    /// A turn of the tests.
    #[derive(Debug)]
    struct Turn;

    impl TurnSpec for Turn {
        const NAME: &'static str = "tests.turn";

        fn period(&self) -> Ticks {
            Ticks::new(5)
        }
    }

    #[test]
    fn a_first_turn_comes_at_the_first_tick_from_now_of_its_ids_remainder() {
        // Unit 7 of a period of 5 takes its first turn in a tick that leaves 7 % 5 = 2: from
        // tick 10, 10 % 5 = 0 waits 2, to 12; from 12 it is 12; from 13, 13 % 5 = 3 waits 4, to
        // 17. A next turn is its tick, whatever the remainder.
        let mut ids = IdAllocator::default();
        let id = (0..=7).map(|_| ids.allocate()).last().unwrap();
        assert_eq!(id.get(), 7);
        let period = Ticks::new(5);
        let first = |now| {
            NextTurn::<Turn>::FIRST
                .due_at(id, period, Tick::new(now))
                .get()
        };
        assert_eq!([first(10), first(12), first(13)], [12, 12, 17]);
        let next = NextTurn::<Turn>::new(Tick::new(11));
        assert_eq!(next.due_at(id, period, Tick::new(13)), Tick::new(11));
    }
}
