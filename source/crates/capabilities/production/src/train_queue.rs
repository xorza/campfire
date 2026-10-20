use std::num::{NonZeroU8, NonZeroUsize};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::{HookContext, Remove};
use bevy_ecs::observer::On;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Query, ResMut, SystemParam};
use bevy_ecs::world::{DeferredWorld, World};
use campfire_actions::{ActionBook, ActionKind};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ByType;
use campfire_players::{PlayersWorld, ResourceAmount};
use campfire_sim::{DueIndex, SimComponent, SimTick, StableId, TimedKept, TimedProcess, TimedRuns};
use campfire_states::{Replication, SentOnChange};
use campfire_units::{Dead, UnitEntity};
use campfire_values::{ActionId, NameId, Rank, UnitType};
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::production_data::ProductionData;

/// A unit's train queue: the trains it was ordered, in order, at most as many as its type's
/// `production` section holds, and the player resources each paid, one run after another in the
/// entries' order. It makes one at a time, the head, whose time runs from the tick it reached the
/// head. Its hook pushes its unit into the index of the queues at each insert, a spawn's and a
/// restore's alike.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[component(on_insert = TrainQueue::pushed)]
pub struct TrainQueue {
    entries: Vec<Queued>,
    /// The tick the head's time ends; none for an empty queue.
    head_done: Option<Tick>,
    paid: Vec<ResourceAmount>,
}

/// A train in a queue: its action, the rank it was ordered at, the time it takes, as its action's
/// rank gave it when it joined, and how many player resources it paid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queued {
    pub action: ActionId,
    pub rank: Rank,
    pub time: Ticks,
    pub paid: u8,
}

/// The index of the queues, as a system that changes a unit's queue takes it: none on a match
/// that finishes no trains, as a predicting client.
#[derive(SystemParam, Debug)]
pub(crate) struct QueueIndex<'w>(Option<ResMut<'w, DueIndex<TrainQueue>>>);

/// The index of the queues, for the queue of the unit `id`, of `entity`, which each change of the
/// queue pushes it into.
#[derive(Debug)]
pub struct QueueDues<'a> {
    index: Option<&'a mut DueIndex<TrainQueue>>,
    id: StableId,
    entity: Entity,
}

impl QueueIndex<'_> {
    /// The index, for the queue of the unit `id`, of `entity`.
    pub(crate) fn of(&mut self, id: StableId, entity: Entity) -> QueueDues<'_> {
        QueueDues {
            index: self.0.as_deref_mut(),
            id,
            entity,
        }
    }
}

impl<'a> QueueDues<'a> {
    /// The index `index`, for the queue of the unit `id`, of `entity`.
    pub(crate) const fn new(
        index: Option<&'a mut DueIndex<TrainQueue>>,
        id: StableId,
        entity: Entity,
    ) -> Self {
        Self { index, id, entity }
    }

    /// Pushes the unit as `queue` makes it due: as its head's time ends.
    pub(crate) fn push(&mut self, queue: &TrainQueue) {
        if let (Some(index), Some(at)) = (self.index.as_deref_mut(), queue.head_done) {
            index.push(at, self.id, self.entity);
        }
    }
}

impl TrainQueue {
    /// Pushes the unit whose queue it is into the index of the queues, at its head's end, in a
    /// match that finishes trains.
    fn pushed(mut world: DeferredWorld<'_>, context: HookContext) {
        let entity = context.entity;
        let unit = world.entity(entity);
        let (Some(queue), Some(&id)) = (unit.get::<Self>(), unit.get::<StableId>()) else {
            return;
        };
        let at = Self::due(&(), (queue, unit.contains::<Dead>()));
        if let (Some(at), Some(mut index)) = (at, world.get_resource_mut::<DueIndex<Self>>()) {
            index.push(at, id, entity);
        }
    }

    /// Pushes the unit of `revived`, which lives again, into the index of the queues, as its
    /// queue, which waited while it was dead, is due again.
    pub(crate) fn revived(
        revived: On<'_, '_, Remove<Dead>>,
        units: Query<'_, '_, (&StableId, &Self)>,
        index: Option<ResMut<'_, DueIndex<Self>>>,
    ) {
        let entity = revived.entity;
        if let (Ok((&id, queue)), Some(mut index)) = (units.get(entity), index) {
            QueueDues::new(Some(&mut index), id, entity).push(queue);
        }
    }

    /// Whether it holds fewer than `capacity` trains.
    pub(crate) fn has_room(&self, capacity: NonZeroU8) -> bool {
        self.entries.len() < usize::from(capacity.get())
    }

    /// The tick its head's time ends; none for an empty queue.
    pub const fn head_done(&self) -> Option<Tick> {
        self.head_done
    }

    pub fn entries(&self) -> &[Queued] {
        &self.entries
    }

    /// Adds `queued`, which paid `paid`, at the end; at the head, its time runs from `now`, and
    /// its unit is due in `dues` as it ends.
    pub fn push(
        &mut self,
        queued: Queued,
        paid: &[ResourceAmount],
        now: Tick,
        dues: &mut QueueDues<'_>,
    ) {
        debug_assert_eq!(usize::from(queued.paid), paid.len());
        if self.entries.is_empty() {
            self.head_done = Some(now.after(queued.time));
        }
        self.entries.push(queued);
        self.paid.extend_from_slice(paid);
        dues.push(self);
    }

    /// The head, when its time ended by `now`.
    pub(crate) fn done(&self, now: Tick) -> Option<Queued> {
        let ended = self.head_done.is_some_and(|done| done <= now);
        self.entries.first().copied().filter(|_| ended)
    }

    /// The player resources the entry at `place` paid; `None` past the queue's end.
    pub(crate) fn paid(&self, place: usize) -> Option<&[ResourceAmount]> {
        self.entries.get(place)?;
        let start = self.paid_before(place);
        Some(&self.paid[start..start + usize::from(self.entries[place].paid)])
    }

    /// Takes away the entry at `place`, one the queue holds, in `now`: the head made, or any
    /// entry cancelled. A head's leaving starts the next one's time at `now`, its unit due in
    /// `dues` as it ends.
    pub fn remove(&mut self, place: usize, now: Tick, dues: &mut QueueDues<'_>) {
        let start = self.paid_before(place);
        let removed = self.entries.remove(place);
        self.paid.drain(start..start + usize::from(removed.paid));
        if place == 0 {
            self.head_done = self.entries.first().map(|next| now.after(next.time));
        }
        dues.push(self);
    }

    /// The count of player resources the entries before `place` paid.
    fn paid_before(&self, place: usize) -> usize {
        self.entries[..place]
            .iter()
            .map(|queued| usize::from(queued.paid))
            .sum()
    }
}

/// A living unit's queue is due as its head's time ends; a dead unit's waits
/// (04-capabilities/00-overview.md, One due index for each timed process).
impl TimedProcess for TrainQueue {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (&'static Self, Has<Dead>);
    type Books = ();

    fn due((): &(), (queue, dead): (&Self, bool)) -> Option<Tick> {
        queue.head_done.filter(|_| !dead)
    }
}

impl SimComponent for TrainQueue {
    const NAME: &'static str = "production.train_queue";
    type Host = UnitEntity;

    // A train the book lacks, or of a rank past its ranks, has no unit to make; a queue of a
    // type that produces nothing, or past what its type holds, breaks the rule of its room; a
    // paid resource the mode lacks, or an amount below 0, has no refund to give; and a head's time
    // ends at most its time after the tick the match runs next, as it started before it.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<ActionBook>();
        let trains = self.entries.iter().all(|queued| {
            book.get(queued.action).is_some_and(|action| {
                action.kind == ActionKind::Train && action.has_rank(queued.rank)
            })
        });
        let producers = world.resource::<ByType<ProductionData>>();
        let capacity = world
            .get::<UnitType>(entity)
            .and_then(|&unit_type| producers.get(unit_type))
            .map(|production| NonZeroUsize::from(production.queue).get());
        let resources = world.declared_resources();
        let paid = self
            .paid
            .iter()
            .all(|paid| paid.resource.index() < resources && paid.amount >= 0);
        let now = world.resource::<SimTick>().start();
        let head = self.entries.first().zip(self.head_done);
        let timed = head.is_none_or(|(head, done)| done <= now.after(head.time));
        trains && capacity.is_some_and(|capacity| self.entries.len() <= capacity) && paid && timed
    }
}

impl Replication for TrainQueue {
    const KIND: DataKind = DataKind::Queues;
    type Sending = SentOnChange;
}

/// A snapshot is untrusted, so a head time without a head or without one, or paid amounts that
/// are not the entries' runs, fail to decode.
impl<'de> Deserialize<'de> for TrainQueue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            entries: Vec<Queued>,
            head_done: Option<Tick>,
            paid: Vec<ResourceAmount>,
        }
        let Fields {
            entries,
            head_done,
            paid,
        } = Fields::deserialize(deserializer)?;
        if entries.is_empty() != head_done.is_none() {
            return Err(D::Error::custom("a head time exactly when a head"));
        }
        let runs: usize = entries.iter().map(|queued| usize::from(queued.paid)).sum();
        if runs != paid.len() {
            return Err(D::Error::custom("an entry's paid run for each entry"));
        }
        Ok(Self {
            entries,
            head_done,
            paid,
        })
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::entity::Entity;
    use campfire_sim::StableId;

    use crate::train_queue::QueueDues;

    impl QueueDues<'_> {
        /// No index, for a queue a test makes outside a match, whose insert pushes its unit.
        pub const fn loose() -> Self {
            Self::new(None, StableId::MATCH, Entity::PLACEHOLDER)
        }
    }
}
