use std::num::NonZeroU8;

use bevy_ecs::component::Component;
use campfire_sim::{SimComponent, Tick, Ticks};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::actions::action_book::ActionId;

/// A unit's train queue: the trains it was ordered, in order, at most `capacity`. It makes one at
/// a time, the head, whose time runs from the tick it reached the head.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrainQueue {
    capacity: NonZeroU8,
    entries: Vec<Queued>,
    /// The tick the head's time ends; none for an empty queue.
    head_done: Option<Tick>,
}

/// A train in a queue: its action, and the rank it was ordered at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queued {
    pub action: ActionId,
    pub rank: u8,
}

impl TrainQueue {
    pub(crate) fn new(capacity: NonZeroU8) -> TrainQueue {
        TrainQueue {
            capacity,
            entries: Vec::with_capacity(usize::from(capacity.get())),
            head_done: None,
        }
    }

    pub(crate) fn has_room(&self) -> bool {
        self.entries.len() < usize::from(self.capacity.get())
    }

    pub fn entries(&self) -> &[Queued] {
        &self.entries
    }

    /// Adds `queued` at the end, a queue with room; at the head, its time of `time` runs from
    /// `now`.
    pub(crate) fn push(&mut self, queued: Queued, now: Tick, time: Ticks) {
        debug_assert!(self.has_room(), "a train joins a queue with room");
        if self.entries.is_empty() {
            self.head_done = Some(now.after(time));
        }
        self.entries.push(queued);
    }

    /// The head, when its time ended by `now`.
    pub(crate) fn done(&self, now: Tick) -> Option<Queued> {
        self.head_done
            .filter(|&done| done <= now)
            .and(self.entries.first().copied())
    }

    /// Takes the head away, made at `now`: the next, of `next_time` when there is one, starts its
    /// time at `now`.
    pub(crate) fn pop(&mut self, now: Tick, next_time: Option<Ticks>) {
        self.entries.remove(0);
        debug_assert_eq!(
            self.entries.is_empty(),
            next_time.is_none(),
            "a next time for a next"
        );
        self.head_done = next_time.map(|time| now.after(time));
    }
}

impl SimComponent for TrainQueue {
    const NAME: &'static str = "production.train_queue";
}

/// A snapshot is untrusted, so a queue past its capacity, or a head time without a head or
/// without one, fails to decode.
impl<'de> Deserialize<'de> for TrainQueue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<TrainQueue, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            capacity: NonZeroU8,
            entries: Vec<Queued>,
            head_done: Option<Tick>,
        }
        let Fields {
            capacity,
            entries,
            head_done,
        } = Fields::deserialize(deserializer)?;
        if entries.len() > usize::from(capacity.get()) {
            return Err(D::Error::custom("no more entries than the queue holds"));
        }
        if entries.is_empty() != head_done.is_none() {
            return Err(D::Error::custom("a head time exactly when a head"));
        }
        Ok(TrainQueue {
            capacity,
            entries,
            head_done,
        })
    }
}
