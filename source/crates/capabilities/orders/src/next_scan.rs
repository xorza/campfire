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

use crate::acquire::Acquire;
use crate::stagger;

/// The tick a unit that scans is due in again, a part of each unit of a type with an idle scan
/// from its spawn: none before its first scan, which comes in the first tick that leaves the
/// remainder of its stable id when divided by its type's period, then a period after each. A scan
/// writes it anew, and its hook pushes its unit into the index of the scans at each insert, a
/// spawn's and a restore's alike.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_insert = NextScan::pushed)]
#[serde(transparent)]
pub struct NextScan(Option<Tick>);

impl NextScan {
    /// Before the unit's first scan.
    pub const FIRST: Self = Self(None);

    pub const fn new(tick: Tick) -> Self {
        Self(Some(tick))
    }

    /// The tick its unit, `id`, of a type that scans each `period`, is due in, as the tick `now`
    /// starts.
    pub(crate) const fn due_at(self, id: StableId, period: Ticks, now: Tick) -> Tick {
        stagger::due_at(self.0, id, period, now)
    }

    /// Pushes its unit into the index of the scans, at its due tick, in a match that scans.
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
        let book = world.get_resource::<ByType<Acquire>>();
        let Some(acquire) = book.and_then(|book| book.get(unit_type)) else {
            return;
        };
        let Some(now) = world.get_resource::<SimTick>().map(|tick| tick.start()) else {
            return;
        };
        let at = next.due_at(id, acquire.period, now);
        if let Some(mut index) = world.get_resource_mut::<DueIndex<Self>>() {
            index.push(at, id, entity);
        }
    }

    /// Pushes the unit of `revived`, which lives again, into the index of the scans.
    pub(crate) fn revived(
        revived: On<'_, '_, Remove<Dead>>,
        units: Query<'_, '_, (&StableId, &UnitType, &Self)>,
        (book, tick): (Option<Res<'_, ByType<Acquire>>>, Res<'_, SimTick>),
        index: Option<ResMut<'_, DueIndex<Self>>>,
    ) {
        let entity = revived.entity;
        let (Ok((&id, &unit_type, &next)), Some(mut index)) = (units.get(entity), index) else {
            return;
        };
        if let Some(acquire) = book.as_ref().and_then(|book| book.get(unit_type)) {
            index.push(next.due_at(id, acquire.period, tick.start()), id, entity);
        }
    }
}

/// A living unit of a type that scans is due at its next scan; a dead one's waits
/// (04-capabilities/00-overview.md, One due index for each timed process).
impl TimedProcess for NextScan {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (
        &'static Self,
        &'static StableId,
        &'static UnitType,
        Has<Dead>,
    );
    type Books = (Option<Res<'static, ByType<Acquire>>>, Res<'static, SimTick>);

    fn due(
        (book, tick): &(Option<Res<'_, ByType<Acquire>>>, Res<'_, SimTick>),
        (next, &id, &unit_type, dead): (&Self, &StableId, &UnitType, bool),
    ) -> Option<Tick> {
        let acquire = book.as_ref()?.get(unit_type).filter(|_| !dead)?;
        Some(next.due_at(id, acquire.period, tick.start()))
    }
}

impl SimComponent for NextScan {
    const NAME: &'static str = "orders.next_scan";
    type Host = UnitEntity;

    // A scan sets the next a period on, so a unit of a type that scans is due at most a period
    // after the tick the match runs next; a later tick would keep it from scanning.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let Some(next) = self.0 else {
            return true;
        };
        let now = world.resource::<SimTick>().start();
        let acquire = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| world.resource::<ByType<Acquire>>().get(unit_type));
        acquire.is_some_and(|acquire| next <= now.after(acquire.period))
    }
}

impl Replication for NextScan {
    const KIND: DataKind = DataKind::Server;
    type Sending = NotSent;
}
