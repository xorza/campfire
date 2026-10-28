use std::any;
use std::fmt;
use std::marker::PhantomData;

use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::lifecycle::{HookContext, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, ReleaseStateQueryData, SingleEntityQueryData};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Query, Res, ResMut, StaticSystemParam};
use bevy_ecs::world::{DeferredWorld, Mut, World};
use campfire_common::Tick;

use crate::prediction_started::PredictionStarted;
use crate::sim_update::{TickEndSystems, end_tick};
use crate::stable_id::StableId;
use crate::state_rewritten::StateRewritten;
use crate::timed_process::{TimedKept, TimedProcess};
use crate::unpredicted::Unpredicted;

/// The units a timed process `P` visits when their time comes, each once, at the earliest tick
/// pushed for it since its last visit: a process pushes a unit where it sets the unit's time, and
/// its pass takes only the units due, by tick, then stable id, so the order of its visits is the
/// same in every world that holds the same state (04-capabilities/00-overview.md, One due index
/// for each timed process). An entry is a visit, never a promise: one whose unit's time moved
/// later, or that went, costs a visit that finds nothing due, and the pass pushes the unit at its
/// new time. A unit is its entity, and an entity that takes the index of one gone takes its entry
/// too. A cache, which builds again from the parts as the state is written from outside the sim;
/// not state.
///
/// A binary heap by tick, then stable id, and the place of each entity's entry by its index:
/// both keep their capacity as entries come and go, so once they held as many units as the
/// match holds at its most, a tick allocates nothing (02-engine-core.md, Allocation audits).
#[derive(Resource)]
pub struct DueIndex<P> {
    heap: Vec<Due>,
    /// The place in `heap` of each entity's entry, by the entity's index; `NONE` where it holds
    /// none.
    places: Vec<u32>,
    process: PhantomData<fn() -> P>,
}

/// A unit due: when, its stable id, which orders two due at one tick, and its entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Due {
    at: Tick,
    id: StableId,
    entity: Entity,
}

impl Due {
    const fn key(self) -> (Tick, StableId) {
        (self.at, self.id)
    }

    const fn unit(self) -> DueUnit {
        DueUnit {
            id: self.id,
            entity: self.entity,
        }
    }
}

/// The index of `P`, for the unit `id`, of `entity`, which each change of the unit's part pushes it
/// into: what a setter of a process kept by its setters holds; none in a match that runs no `P`,
/// as a predicting client may.
#[derive(Debug)]
pub struct UnitDues<'a, P> {
    index: Option<&'a mut DueIndex<P>>,
    id: StableId,
    entity: Entity,
}

/// A unit a pass visits as its time came.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueUnit {
    pub id: StableId,
    pub entity: Entity,
}

impl<P> DueIndex<P> {
    const NONE: u32 = u32::MAX;

    /// Visits the unit `id`, of `entity`, at `at`, or at the tick it is due at when that comes
    /// first.
    pub fn push(&mut self, at: Tick, id: StableId, entity: Entity) {
        debug_assert_ne!(
            entity,
            Entity::PLACEHOLDER,
            "a unit's entity, as the index keeps a place for each entity index below the largest"
        );
        let due = Due { at, id, entity };
        let Some(place) = self.place(entity.index_u32()) else {
            let place = self.heap.len();
            self.heap.push(due);
            let slot = entity.index_u32() as usize;
            if slot >= self.places.len() {
                self.places.resize(slot + 1, Self::NONE);
            }
            self.places[slot] = u32::try_from(place).expect("an index of fewer than 2³² units");
            self.sift_up(place);
            return;
        };
        let held = self.heap[place];
        if held.entity == entity && held.at <= at {
            return;
        }
        self.heap[place] = due;
        self.sift(place);
    }

    /// Moves each unit due at `now` or before into `due`, out of the index, by tick, then stable
    /// id.
    pub fn take_due(&mut self, now: Tick, due: &mut Vec<DueUnit>) {
        while let Some(&first) = self.heap.first()
            && first.at <= now
        {
            self.remove_at(0);
            due.push(first.unit());
        }
    }

    /// Moves each unit due at `now` or before that `takes` takes into `due`, out of the index, by
    /// tick, then stable id; the others stay, for another pass of the same index.
    pub fn take_due_if(
        &mut self,
        now: Tick,
        mut takes: impl FnMut(DueUnit) -> bool,
        due: &mut Vec<DueUnit>,
    ) {
        let from = due.len();
        self.peek_due(now, due);
        let mut kept = from;
        for at in from..due.len() {
            let unit = due[at];
            if takes(unit) {
                due[kept] = unit;
                kept += 1;
            }
        }
        due.truncate(kept);
        for unit in &due[from..] {
            self.forget(unit.entity);
        }
    }

    /// Takes the unit of `entity` out of the index.
    pub fn forget(&mut self, entity: Entity) {
        if let Some(place) = self.place(entity.index_u32())
            && self.heap[place].entity == entity
        {
            self.remove_at(place);
        }
    }

    /// Appends each unit due at `now` or before to `due`, by tick, then stable id, which the index
    /// keeps. A walk of the heap from its root that stops at each entry not due, with `due` its
    /// queue, so it reads only the units due and a few past them.
    pub fn peek_due(&self, now: Tick, due: &mut Vec<DueUnit>) {
        let from = due.len();
        if let Some(first) = self.heap.first()
            && first.at <= now
        {
            due.push(first.unit());
        }
        let mut next = from;
        while let Some(&unit) = due.get(next) {
            let place = self.places[unit.entity.index_u32() as usize] as usize;
            for child in [2 * place + 1, 2 * place + 2] {
                if let Some(held) = self.heap.get(child)
                    && held.at <= now
                {
                    due.push(held.unit());
                }
            }
            next += 1;
        }
        due[from..].sort_unstable_by_key(|unit| {
            self.heap[self.places[unit.entity.index_u32() as usize] as usize].key()
        });
    }

    /// Each unit it holds, at any tick, in no order: for a pass whose result no order changes.
    pub fn units(&self) -> impl Iterator<Item = DueUnit> + '_ {
        self.heap.iter().map(|held| held.unit())
    }

    /// The tick the unit of `entity` is due at, when the index holds it.
    pub fn due_at(&self, entity: Entity) -> Option<Tick> {
        self.place(entity.index_u32())
            .map(|place| self.heap[place])
            .filter(|held| held.entity == entity)
            .map(|held| held.at)
    }

    /// How many units it holds.
    pub const fn len(&self) -> usize {
        self.heap.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Forgets every unit.
    fn clear(&mut self) {
        self.heap.clear();
        self.places.fill(Self::NONE);
    }

    /// The place of the entry of the entity of index `slot`, of any generation.
    fn place(&self, slot: u32) -> Option<usize> {
        let place = *self.places.get(slot as usize)?;
        (place != Self::NONE).then_some(place as usize)
    }

    /// Takes the entry at `place` out of the heap.
    fn remove_at(&mut self, place: usize) {
        let gone = self.heap.swap_remove(place);
        self.places[gone.entity.index_u32() as usize] = Self::NONE;
        if place < self.heap.len() {
            self.set_place(place);
            self.sift(place);
        }
    }

    /// Moves the entry at `place`, whose key changed, up or down to where the heap holds.
    fn sift(&mut self, place: usize) {
        if place > 0 && self.heap[place].key() < self.heap[(place - 1) / 2].key() {
            self.sift_up(place);
        } else {
            self.sift_down(place);
        }
    }

    fn sift_up(&mut self, mut place: usize) {
        while place > 0 {
            let parent = (place - 1) / 2;
            if self.heap[parent].key() <= self.heap[place].key() {
                break;
            }
            self.swap(place, parent);
            place = parent;
        }
    }

    fn sift_down(&mut self, mut place: usize) {
        loop {
            let [left, right] = [2 * place + 1, 2 * place + 2];
            let mut least = place;
            for child in [left, right] {
                if child < self.heap.len() && self.heap[child].key() < self.heap[least].key() {
                    least = child;
                }
            }
            if least == place {
                return;
            }
            self.swap(place, least);
            place = least;
        }
    }

    fn swap(&mut self, a: usize, b: usize) {
        self.heap.swap(a, b);
        self.set_place(a);
        self.set_place(b);
    }

    /// Writes the place of the entry at `place` into `places`.
    fn set_place(&mut self, place: usize) {
        let slot = self.heap[place].entity.index_u32() as usize;
        self.places[slot] = place as u32;
    }
}

impl<'a, P> UnitDues<'a, P> {
    /// The index `index`, for the unit `id`, of `entity`.
    pub const fn new(index: &'a mut DueIndex<P>, id: StableId, entity: Entity) -> Self {
        Self {
            index: Some(index),
            id,
            entity,
        }
    }

    /// The index `index`, when the match runs `P`, for the unit `id`, of `entity`.
    pub const fn of(index: Option<&'a mut DueIndex<P>>, id: StableId, entity: Entity) -> Self {
        Self { index, id, entity }
    }

    /// The same index, for a shorter borrow.
    pub fn reborrow(&mut self) -> UnitDues<'_, P> {
        UnitDues {
            index: self.index.as_deref_mut(),
            id: self.id,
            entity: self.entity,
        }
    }

    /// Pushes the unit at `at`, the tick its change makes it due; none pushes nothing.
    pub fn push(&mut self, at: Option<Tick>) {
        if let (Some(index), Some(at)) = (self.index.as_deref_mut(), at) {
            index.push(at, self.id, self.entity);
        }
    }
}

impl<P: TimedProcess> UnitDues<'_, P> {
    /// Runs `change` on `world`, with the index of `P` for the unit `id`, of `entity`, as an
    /// exclusive system or an effect changes the unit.
    pub fn with<R>(
        world: &mut World,
        id: StableId,
        entity: Entity,
        change: impl FnOnce(&mut World, &mut UnitDues<'_, P>) -> R,
    ) -> R {
        world.resource_scope(|world, mut index: Mut<'_, DueIndex<P>>| {
            change(world, &mut UnitDues::new(&mut index, id, entity))
        })
    }
}

impl<P> DueIndex<P>
where
    P: TimedProcess<Books = ()>,
    P::Parts: ReleaseStateQueryData + SingleEntityQueryData,
{
    /// Pushes the unit whose part comes into the index of `P`, at the tick `P::due` reads from
    /// its parts, in a match that runs `P`: the insert hook of the part of a process kept by its
    /// setters that reads no book, so no process restates its time in a hook of its own.
    pub fn pushed(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let unit = world.entity(entity);
        let (Some(&id), Some(parts)) = (
            unit.get::<StableId>(),
            unit.get_components::<P::Parts>().ok(),
        ) else {
            return;
        };
        let Some(at) = P::due(&(), parts) else {
            return;
        };
        if let Some(mut index) = world.get_resource_mut::<Self>() {
            index.push(at, id, entity);
        }
    }
}

impl<P: TimedProcess> DueIndex<P> {
    /// Adds what keeps the index of `P` true in `world`: the observers that build it again as
    /// the state is rewritten and push a unit a client starts to predict or that comes back into
    /// the world, and, in a debug build, the check at each tick's end that each unit due is held,
    /// in `schedule`.
    pub fn track(world: &mut World, schedule: &mut Schedule) {
        world.add_observer(Self::rebuild);
        world.add_observer(Self::predicted);
        world.add_observer(Self::enabled);
        if cfg!(debug_assertions) && P::KEPT == TimedKept::BySetters {
            schedule.add_systems(Self::audit.in_set(TickEndSystems).before(end_tick));
        }
    }

    /// Builds the index again from the parts of each unit the sim runs, as the state was written
    /// from outside the sim.
    fn rebuild(
        _: On<'_, '_, StateRewritten>,
        books: StaticSystemParam<'_, '_, P::Books>,
        units: Query<'_, '_, (Entity, &StableId, P::Parts)>,
        mut index: ResMut<'_, Self>,
    ) {
        index.clear();
        for (entity, &id, parts) in &units {
            if let Some(at) = P::due(&books, parts) {
                index.push(at, id, entity);
            }
        }
    }

    /// Pushes a unit a predicting client starts to predict, whose parts replication wrote.
    fn predicted(
        started: On<'_, '_, PredictionStarted>,
        books: StaticSystemParam<'_, '_, P::Books>,
        units: Query<'_, '_, (&StableId, P::Parts), Allow<Unpredicted>>,
        index: Option<ResMut<'_, Self>>,
    ) {
        let entity = started.entity;
        let (Some(mut index), Ok((&id, parts))) = (index, units.get(entity)) else {
            return;
        };
        if let Some(at) = P::due(&books, parts) {
            index.push(at, id, entity);
        }
    }

    /// Pushes a unit that comes back into the world as its `Disabled` goes, whose entry a pass
    /// that found no unit dropped while it was out (transports.md, A rider's timers wait and catch
    /// up).
    fn enabled(
        enabled: On<'_, '_, Remove<Disabled>>,
        books: StaticSystemParam<'_, '_, P::Books>,
        units: Query<'_, '_, (&StableId, P::Parts), Allow<Disabled>>,
        index: Option<ResMut<'_, Self>>,
    ) {
        let entity = enabled.entity;
        let (Some(mut index), Ok((&id, parts))) = (index, units.get(entity)) else {
            return;
        };
        if let Some(at) = P::due(&books, parts) {
            index.push(at, id, entity);
        }
    }

    /// Checks, as each tick ends and before it advances, that each unit the sim runs that has a
    /// time to come is held at that time or before: a pass that set a time and pushed none is a
    /// logic error.
    fn audit(
        books: StaticSystemParam<'_, '_, P::Books>,
        units: Query<'_, '_, (Entity, &StableId, P::Parts)>,
        index: Res<'_, Self>,
    ) {
        for (entity, &id, parts) in &units {
            let Some(at) = P::due(&books, parts) else {
                continue;
            };
            let held = index.due_at(entity);
            assert!(
                held.is_some_and(|held| held <= at),
                "{} is due to {} at {at:?} but its index holds {held:?}: a pass set its time and \
                 pushed none",
                id.get(),
                any::type_name::<P>()
            );
        }
    }
}

impl<P> Default for DueIndex<P> {
    fn default() -> Self {
        Self {
            heap: Vec::new(),
            places: Vec::new(),
            process: PhantomData,
        }
    }
}

impl<P> fmt::Debug for DueIndex<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DueIndex")
            .field("heap", &self.heap)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;

    use crate::due_index::UnitDues;
    use crate::stable_id::StableId;

    impl<P> UnitDues<'_, P> {
        /// No index, for a part a test changes outside a match.
        pub const fn loose() -> Self {
            Self::of(None, StableId::MATCH, Entity::PLACEHOLDER)
        }
    }
}

#[cfg(test)]
mod tests;
