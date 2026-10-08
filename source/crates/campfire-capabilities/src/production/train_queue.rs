use std::num::NonZeroU8;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::production::production_data::ProductionData;
use crate::units::action_id::ActionId;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

/// A unit's train queue: the trains it was ordered, in order, at most as many as its type's
/// `production` section holds, and the player resources each paid, one run after another in the
/// entries' order. It makes one at a time, the head, whose time runs from the tick it reached the
/// head.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
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
    pub rank: u8,
    pub time: Ticks,
    pub paid: u8,
}

impl TrainQueue {
    /// Whether it holds fewer than `capacity` trains.
    pub(crate) fn has_room(&self, capacity: NonZeroU8) -> bool {
        self.entries.len() < usize::from(capacity.get())
    }

    pub fn entries(&self) -> &[Queued] {
        &self.entries
    }

    /// Adds `queued`, which paid `paid`, at the end; at the head, its time runs from `now`.
    pub(crate) fn push(&mut self, queued: Queued, paid: &[ResourceAmount], now: Tick) {
        debug_assert_eq!(usize::from(queued.paid), paid.len());
        if self.entries.is_empty() {
            self.head_done = Some(now.after(queued.time));
        }
        self.entries.push(queued);
        self.paid.extend_from_slice(paid);
    }

    /// The head, when its time ended by `now`.
    pub(crate) fn done(&self, now: Tick) -> Option<Queued> {
        self.head_done
            .filter(|&done| done <= now)
            .and(self.entries.first().copied())
    }

    /// The player resources the entry at `place` paid; `None` past the queue's end.
    pub(crate) fn paid(&self, place: usize) -> Option<&[ResourceAmount]> {
        self.entries.get(place)?;
        let start = self.paid_before(place);
        Some(&self.paid[start..start + usize::from(self.entries[place].paid)])
    }

    /// Takes away the entry at `place`, one the queue holds, in `now`: the head made, or any
    /// entry cancelled. A head's leaving starts the next one's time at `now`.
    pub(crate) fn remove(&mut self, place: usize, now: Tick) {
        let start = self.paid_before(place);
        let removed = self.entries.remove(place);
        self.paid.drain(start..start + usize::from(removed.paid));
        if place == 0 {
            self.head_done = self.entries.first().map(|next| now.after(next.time));
        }
    }

    /// The count of player resources the entries before `place` paid.
    fn paid_before(&self, place: usize) -> usize {
        self.entries[..place]
            .iter()
            .map(|queued| usize::from(queued.paid))
            .sum()
    }
}

impl SimComponent for TrainQueue {
    const NAME: &'static str = "production.train_queue";

    // A train the book lacks, or of a rank past its ranks, has no unit to make; a queue of a
    // type that produces nothing, or past what its type holds, breaks the rule of its room; a
    // paid resource the mode lacks, or an amount below 0, has no refund to give.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let book = world.get_resource::<ActionBook>();
        let trains = self.entries.iter().all(|queued| {
            book.and_then(|book| book.get(queued.action))
                .is_some_and(|action| {
                    action.kind.kind() == ActionKind::Train && action.has_rank(queued.rank)
                })
        });
        let producers = world.get_resource::<ByType<ProductionData>>();
        let capacity = world
            .get::<UnitType>(entity)
            .zip(producers)
            .and_then(|(&unit_type, producers)| producers.get(unit_type))
            .map(|production| usize::from(production.queue.get()));
        let resources = world
            .get_resource::<PlayerResources>()
            .map_or(0, PlayerResources::resources);
        let paid = self
            .paid
            .iter()
            .all(|paid| paid.resource.index() < resources && paid.amount >= 0);
        trains && capacity.is_some_and(|capacity| self.entries.len() <= capacity) && paid
    }
}

/// A snapshot is untrusted, so a head time without a head or without one, or paid amounts that
/// are not the entries' runs, fail to decode.
impl<'de> Deserialize<'de> for TrainQueue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TrainQueue, D::Error> {
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
        Ok(TrainQueue {
            entries,
            head_done,
            paid,
        })
    }
}
