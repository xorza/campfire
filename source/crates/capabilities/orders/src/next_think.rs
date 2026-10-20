use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Query, Res, ResMut};
use bevy_ecs::world::{DeferredWorld, World};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ByType;
use campfire_sim::{DueIndex, SimComponent, SimTick, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{NotSent, Replication};
use campfire_units::{Dead, UnitEntity};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::ai::Ai;

/// The tick a unit that thinks is due in again, a part of each unit of a type with AI from its
/// spawn: none before its first think, which comes in the first tick that leaves the remainder of
/// its stable id when divided by its type's period. A unit whose call found the think pool spent
/// keeps its tick, so it stays due, and goes first in the next tick, before the units due later.
/// A think writes it anew, and its hook pushes its unit into the index of the thinks at each
/// insert, a spawn's and a restore's alike.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_insert = NextThink::pushed)]
#[serde(transparent)]
pub struct NextThink(Option<Tick>);

impl NextThink {
    /// Before the unit's first think.
    pub const FIRST: Self = Self(None);

    pub const fn new(tick: Tick) -> Self {
        Self(Some(tick))
    }

    pub const fn get(self) -> Option<Tick> {
        self.0
    }

    /// The tick its unit, `id`, of a type that thinks each `period`, is due in, as the tick
    /// `now` starts: its next, or before its first, the first tick from `now` that leaves the
    /// remainder of its stable id when divided by the period.
    pub(crate) const fn due_at(self, id: StableId, period: Ticks, now: Tick) -> Tick {
        if let Some(next) = self.0 {
            return next;
        }
        let period = period.get();
        let wait = (id.get() % period + period - now.get() % period) % period;
        Tick::new(now.get() + wait)
    }

    /// Pushes its unit into the index of the thinks, at its due tick, in a match that thinks.
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
        let book = world.get_resource::<ByType<Ai>>();
        let Some(ai) = book.and_then(|book| book.get(unit_type)) else {
            return;
        };
        let Some(now) = world.get_resource::<SimTick>().map(|tick| tick.start()) else {
            return;
        };
        let at = next.due_at(id, ai.period, now);
        if let Some(mut index) = world.get_resource_mut::<DueIndex<Self>>() {
            index.push(at, id, entity);
        }
    }

    /// Pushes the unit of `revived`, which lives again, into the index of the thinks, as its AI,
    /// which waited while it was dead, is due again.
    pub(crate) fn revived(
        revived: On<'_, '_, Remove<Dead>>,
        units: Query<'_, '_, (&StableId, &UnitType, &Self)>,
        (book, tick): (Option<Res<'_, ByType<Ai>>>, Res<'_, SimTick>),
        index: Option<ResMut<'_, DueIndex<Self>>>,
    ) {
        let entity = revived.entity;
        let (Ok((&id, &unit_type, &next)), Some(mut index)) = (units.get(entity), index) else {
            return;
        };
        if let Some(ai) = book.as_ref().and_then(|book| book.get(unit_type)) {
            index.push(next.due_at(id, ai.period, tick.start()), id, entity);
        }
    }
}

/// A living unit of a type that thinks is due at its next think; a dead one's AI waits
/// (04-capabilities/00-overview.md, One due index for each timed process).
impl TimedProcess for NextThink {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (
        &'static Self,
        &'static StableId,
        &'static UnitType,
        Has<Dead>,
    );
    type Books = (Option<Res<'static, ByType<Ai>>>, Res<'static, SimTick>);

    fn due(
        (book, tick): &(Option<Res<'_, ByType<Ai>>>, Res<'_, SimTick>),
        (next, &id, &unit_type, dead): (&Self, &StableId, &UnitType, bool),
    ) -> Option<Tick> {
        let ai = book.as_ref()?.get(unit_type).filter(|_| !dead)?;
        Some(next.due_at(id, ai.period, tick.start()))
    }
}

impl SimComponent for NextThink {
    const NAME: &'static str = "orders.next_think";
    type Host = UnitEntity;

    // A think sets the next a period on, so a unit of a type with AI is due at most a period
    // after the tick the match runs next; a later tick would keep its AI from thinking.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let Some(next) = self.0 else {
            return true;
        };
        let now = world.resource::<SimTick>().start();
        let ai = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| world.resource::<ByType<Ai>>().get(unit_type));
        ai.is_some_and(|ai| next <= now.after(ai.period))
    }
}

impl Replication for NextThink {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_first_think_comes_at_the_first_tick_from_now_of_its_ids_remainder() {
        // Unit 7 of a period of 5 thinks first in a tick that leaves 7 % 5 = 2: from tick 10,
        // 10 % 5 = 0 waits 2, to 12; from 12 it is 12; from 13, 13 % 5 = 3 waits 4, to 17. A
        // next think is its tick, whatever the remainder.
        let mut ids = IdAllocator::default();
        let id = (0..=7).map(|_| ids.allocate()).last().unwrap();
        assert_eq!(id.get(), 7);
        let period = Ticks::new(5);
        let first = |now| NextThink::FIRST.due_at(id, period, Tick::new(now)).get();
        assert_eq!([first(10), first(12), first(13)], [12, 12, 17]);
        let next = NextThink::new(Tick::new(11));
        assert_eq!(next.due_at(id, period, Tick::new(13)), Tick::new(11));
    }
}
