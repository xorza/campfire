use std::num::{NonZeroU8, NonZeroUsize};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::Remove;
use bevy_ecs::observer::On;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Query, ResMut, SystemParam};
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionKind};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_players::{PlayersWorld, ResourceAmount};
use campfire_sim::{
    DueIndex, SimComponent, SimTick, StableId, TimedKept, TimedProcess, TimedRuns, UnitDues,
};
use campfire_states::{Replication, SentPredicted};
use campfire_units::{Dead, UnitEntity};
use campfire_values::{ActionId, NameId, Rank, UnitType};
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::doors::Doors;
use crate::power_book::PowerBook;
use crate::producer_spec::ProducerSpec;
use crate::queue_pace::QueuePace;

/// A unit's train queue: the trains it was ordered, in order, at most as many as its type's
/// `production` section holds, and the player resources each paid, one run after another in the
/// entries' order. It makes one at a time, the head, whose time runs from the tick it reached the
/// head at its pace, which its player's power may change while it runs
/// (04-capabilities/production.md, A train's end follows the speed). Its hook pushes its unit into
/// the index of the queues at each insert, a spawn's and a restore's alike.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[component(on_insert = DueIndex::<Self>::pushed)]
pub struct TrainQueue {
    entries: Vec<Queued>,
    /// The head's times; none for an empty queue.
    head: Option<HeadTimes>,
    paid: Vec<ResourceAmount>,
}

/// When a queue's head reached the head, and the tick its time at its pace ends, no sooner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct HeadTimes {
    start: Tick,
    done: Tick,
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

impl QueueIndex<'_> {
    /// The index, for the queue of the unit `id`, of `entity`.
    pub(crate) fn of(&mut self, id: StableId, entity: Entity) -> UnitDues<'_, TrainQueue> {
        UnitDues::of(self.0.as_deref_mut(), id, entity)
    }
}

impl TrainQueue {
    /// Pushes the unit of `revived`, which lives again, into the index of the queues, as its
    /// queue, which waited while it was dead, is due again.
    pub(crate) fn revived(
        revived: On<'_, '_, Remove<Dead>>,
        units: Query<'_, '_, (&StableId, &Self)>,
        index: Option<ResMut<'_, DueIndex<Self>>>,
    ) {
        let entity = revived.entity;
        if let (Ok((&id, queue)), Some(mut index)) = (units.get(entity), index) {
            UnitDues::new(&mut index, id, entity).push(queue.head_done());
        }
    }

    /// Whether it holds fewer than `capacity` trains.
    pub(crate) fn has_room(&self, capacity: NonZeroU8) -> bool {
        self.entries.len() < usize::from(capacity.get())
    }

    /// Its head's times; none for an empty queue.
    pub const fn head(&self) -> Option<HeadTimes> {
        self.head
    }

    /// The tick its head's time ends; none for an empty queue.
    pub const fn head_done(&self) -> Option<Tick> {
        match self.head {
            Some(head) => Some(head.done),
            None => None,
        }
    }

    pub fn entries(&self) -> &[Queued] {
        &self.entries
    }

    /// Adds `queued`, which paid `paid`, at the end; at the head, its time at `pace` runs from
    /// `now`, and its unit is due in `dues` as it ends.
    pub fn push(
        &mut self,
        queued: Queued,
        paid: &[ResourceAmount],
        now: Tick,
        pace: QueuePace<'_>,
        dues: &mut UnitDues<'_, Self>,
    ) {
        debug_assert_eq!(usize::from(queued.paid), paid.len());
        if self.entries.is_empty() {
            self.head = Some(HeadTimes::from(now, pace.time(queued.action, queued.time)));
        }
        self.entries.push(queued);
        self.paid.extend_from_slice(paid);
        dues.push(self.head_done());
    }

    /// Its head's times again at `pace`, from the tick it reached the head, as its player's power
    /// changed; none for no head, or times that stay.
    pub(crate) fn repaced(&self, pace: QueuePace<'_>) -> Option<HeadTimes> {
        let (head, first) = (self.head?, self.entries.first()?);
        let timed = HeadTimes::from(head.start, pace.time(first.action, first.time));
        (timed != head).then_some(timed)
    }

    /// Takes `head`, times `repaced` gave its head, its unit due in `dues` as it now ends, at once
    /// if that end passed.
    pub(crate) fn retime(&mut self, head: HeadTimes, dues: &mut UnitDues<'_, Self>) {
        debug_assert_eq!(
            self.head.map(|held| held.start),
            Some(head.start),
            "a head's times again keep its start"
        );
        self.head = Some(head);
        dues.push(self.head_done());
    }

    /// The head, when its time ended by `now`.
    pub(crate) fn done(&self, now: Tick) -> Option<Queued> {
        let ended = self.head.is_some_and(|head| head.done <= now);
        self.entries.first().copied().filter(|_| ended)
    }

    /// The player resources the entry at `place` paid; `None` past the queue's end.
    pub(crate) fn paid(&self, place: usize) -> Option<&[ResourceAmount]> {
        self.entries.get(place)?;
        let start = self.paid_before(place);
        Some(&self.paid[start..start + usize::from(self.entries[place].paid)])
    }

    /// Takes away the entry at `place`, one the queue holds, in `now`: the head made, or any
    /// entry cancelled. A head's leaving starts the next one's time at `pace` at `now`, its unit
    /// due in `dues` as it ends.
    pub fn remove(
        &mut self,
        place: usize,
        now: Tick,
        pace: QueuePace<'_>,
        dues: &mut UnitDues<'_, Self>,
    ) {
        let start = self.paid_before(place);
        let removed = self.entries.remove(place);
        self.paid.drain(start..start + usize::from(removed.paid));
        if place == 0 {
            let next = self.entries.first();
            self.head = next.map(|next| HeadTimes::from(now, pace.time(next.action, next.time)));
        }
        dues.push(self.head_done());
    }

    /// The count of player resources the entries before `place` paid.
    fn paid_before(&self, place: usize) -> usize {
        self.entries[..place]
            .iter()
            .map(|queued| usize::from(queued.paid))
            .sum()
    }
}

/// A living unit's queue is due as its head's time ends, or as the door its head's unit waits for
/// opens; a dead unit's waits (04-capabilities/00-overview.md, One due index for each timed
/// process; 04-capabilities/production.md, A producer's door opens before its unit leaves).
impl TimedProcess for TrainQueue {
    const RUNS: TimedRuns = TimedRuns::Server;
    const KEPT: TimedKept = TimedKept::BySetters;
    type Parts = (&'static Self, Has<Dead>, Option<&'static Doors>);
    type Books = ();

    fn due((): &(), (queue, dead, doors): (&Self, bool, Option<&Doors>)) -> Option<Tick> {
        let done = queue.head_done().filter(|_| !dead)?;
        Some(
            doors
                .and_then(Doors::last_open_at)
                .map_or(done, |open_at| done.max(open_at)),
        )
    }
}

impl SimComponent for TrainQueue {
    const NAME: &'static str = "production.train_queue";
    type Host = UnitEntity;

    // A train the book lacks, or of a rank past its ranks, has no unit to make; a queue of a
    // type that produces nothing, or past what its type holds, breaks the rule of its room; a
    // paid resource the mode lacks, or an amount below 0, has no refund to give; and a head
    // started by the tick the match runs next, and ends no sooner than its own time after its
    // start and no later than that time at the slowest the mode's power speed goes.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.resource::<ActionBook>();
        let trains = self.entries.iter().all(|queued| {
            book.get(queued.action).is_some_and(|action| {
                matches!(action.kind, ActionKind::Train | ActionKind::Research)
                    && action.has_rank(queued.rank)
            })
        });
        let producers = world.resource::<ById<UnitType, ProducerSpec>>();
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
        let speed = world.resource::<PowerBook>().speed();
        let head = self.entries.first().zip(self.head);
        let timed = head.is_none_or(|(first, head)| {
            let slowest = speed.map_or(first.time, |speed| speed.slowest(first.time));
            head.start <= now
                && HeadTimes::from(head.start, first.time).done <= head.done
                && head.done <= HeadTimes::from(head.start, slowest).done
        });
        trains && capacity.is_some_and(|capacity| self.entries.len() <= capacity) && paid && timed
    }
}

impl Replication for TrainQueue {
    const KIND: DataKind = DataKind::Queues;
    type Sending = SentPredicted;
}

impl HeadTimes {
    /// The times of a head that reached the head at `start` and takes `time`; a time past the
    /// last tick ends at it, which no match reaches.
    pub const fn from(start: Tick, time: Ticks) -> Self {
        Self {
            start,
            done: Tick::new(start.get().saturating_add(time.get())),
        }
    }

    pub const fn start(self) -> Tick {
        self.start
    }

    pub const fn done(self) -> Tick {
        self.done
    }
}

/// A snapshot is untrusted, so times that end before they start fail to decode.
impl<'de> Deserialize<'de> for HeadTimes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            start: Tick,
            done: Tick,
        }
        let Fields { start, done } = Fields::deserialize(deserializer)?;
        if done < start {
            return Err(D::Error::custom("a head ends no sooner than it starts"));
        }
        Ok(Self { start, done })
    }
}

/// A snapshot is untrusted, so head times without a head or a head without them, or paid amounts
/// that are not the entries' runs, fail to decode.
impl<'de> Deserialize<'de> for TrainQueue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            entries: Vec<Queued>,
            head: Option<HeadTimes>,
            paid: Vec<ResourceAmount>,
        }
        let Fields {
            entries,
            head,
            paid,
        } = Fields::deserialize(deserializer)?;
        if entries.is_empty() != head.is_none() {
            return Err(D::Error::custom("head times exactly when a head"));
        }
        let runs: usize = entries.iter().map(|queued| usize::from(queued.paid)).sum();
        if runs != paid.len() {
            return Err(D::Error::custom("an entry's paid run for each entry"));
        }
        Ok(Self {
            entries,
            head,
            paid,
        })
    }
}

#[cfg(test)]
mod tests {
    use campfire_codec::{Binary, BinaryError};
    use campfire_values::ResourceId;

    use super::*;

    #[test]
    fn a_queue_decodes_only_with_head_times_exactly_when_it_has_a_head_in_order_and_its_paid_runs()
    {
        #[derive(Debug, Serialize)]
        struct Fields {
            entries: Vec<Queued>,
            head: Option<HeadTimes>,
            paid: Vec<ResourceAmount>,
        }
        let decode = |entries: Vec<Queued>, done: Option<u64>, paid: Vec<ResourceAmount>| {
            let head = done.map(|done| HeadTimes {
                start: Tick::new(5),
                done: Tick::new(done),
            });
            let fields = Fields {
                entries,
                head,
                paid,
            };
            Binary::decode::<TrainQueue>(&Binary::encode(&fields))
        };
        let gold = ResourceId::new(0);
        let five = ResourceAmount {
            resource: gold,
            amount: 5,
        };
        let queued = Queued {
            action: ActionId::new(0),
            rank: Rank::FIRST,
            time: Ticks::new(30),
            paid: 1,
        };
        // Pushed at tick 5, a train of 30 ticks, which paid 5 gold, is done at tick 35.
        let mut queue = TrainQueue::default();
        queue.push(
            queued,
            &[five],
            Tick::new(5),
            QueuePace::Own,
            &mut UnitDues::loose(),
        );
        assert_eq!(decode(vec![queued], Some(35), vec![five]), Ok(queue));
        assert_eq!(decode(vec![], None, vec![]), Ok(TrainQueue::default()));
        // A head that ends before it starts, at 4 of a start at 5.
        assert_eq!(
            decode(vec![queued], Some(4), vec![five]).err(),
            Some(BinaryError::REFUSED)
        );
        assert_eq!(
            decode(vec![], Some(35), vec![]).err(),
            Some(BinaryError::REFUSED)
        );
        assert_eq!(
            decode(vec![queued], None, vec![five]).err(),
            Some(BinaryError::REFUSED)
        );
        // A paid run longer or shorter than the entries say.
        assert_eq!(
            decode(vec![queued], Some(35), vec![]).err(),
            Some(BinaryError::REFUSED)
        );
        assert_eq!(
            decode(vec![queued], Some(35), vec![five, five]).err(),
            Some(BinaryError::REFUSED)
        );
    }
}
