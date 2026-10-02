use std::num::NonZeroU8;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_math::{Tick, Ticks};
use campfire_sim::SimComponent;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::production::production_data::ProductionData;
use crate::units::action_id::ActionId;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

/// A unit's train queue: the trains it was ordered, in order, at most as many as its type's
/// `production` section holds. It makes one at a time, the head, whose time runs from the tick it
/// reached the head.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TrainQueue {
    entries: Vec<Queued>,
    /// The tick the head's time ends; none for an empty queue.
    head_done: Option<Tick>,
}

/// A train in a queue: its action, the rank it was ordered at, and the time it takes, as its
/// action's rank gave it when it joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queued {
    pub action: ActionId,
    pub rank: u8,
    pub time: Ticks,
}

impl TrainQueue {
    /// Whether it holds fewer than `capacity` trains.
    pub(crate) fn has_room(&self, capacity: NonZeroU8) -> bool {
        self.entries.len() < usize::from(capacity.get())
    }

    pub fn entries(&self) -> &[Queued] {
        &self.entries
    }

    /// Adds `queued` at the end; at the head, its time runs from `now`.
    pub(crate) fn push(&mut self, queued: Queued, now: Tick) {
        if self.entries.is_empty() {
            self.head_done = Some(now.after(queued.time));
        }
        self.entries.push(queued);
    }

    /// The head, when its time ended by `now`.
    pub(crate) fn done(&self, now: Tick) -> Option<Queued> {
        self.head_done
            .filter(|&done| done <= now)
            .and(self.entries.first().copied())
    }

    /// Takes the head away, made at `now`: the next, if one is, starts its time at `now`.
    pub(crate) fn pop(&mut self, now: Tick) {
        self.entries.remove(0);
        self.head_done = self.entries.first().map(|next| now.after(next.time));
    }
}

impl SimComponent for TrainQueue {
    const NAME: &'static str = "production.train_queue";

    // A train the book lacks, or of a rank past its ranks, has no unit to make; a queue of a
    // type that produces nothing, or past what its type holds, breaks the rule of its room.
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
        trains && capacity.is_some_and(|capacity| self.entries.len() <= capacity)
    }
}

/// A snapshot is untrusted, so a head time without a head or without one fails to decode.
impl<'de> Deserialize<'de> for TrainQueue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TrainQueue, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            entries: Vec<Queued>,
            head_done: Option<Tick>,
        }
        let Fields { entries, head_done } = Fields::deserialize(deserializer)?;
        if entries.is_empty() != head_done.is_none() {
            return Err(D::Error::custom("a head time exactly when a head"));
        }
        Ok(TrainQueue { entries, head_done })
    }
}
