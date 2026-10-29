use std::collections::VecDeque;
use std::fmt::Debug;

use bevy_ecs::world::World;
use campfire_names::{TypedItem, TypedList};

use crate::scripts::frame::Frame;

/// An effect a call queues: it applies itself when the call returns, in the order queued among
/// every type's.
pub trait Effect: Debug + 'static {
    /// What the effect does to the match, which says which roles' calls may queue it.
    const KIND: EffectKind;

    /// Applies the effect, which the call in `frame` queued, in the call's tick.
    fn apply(self, world: &mut World, frame: &mut Frame);
}

/// What an effect does to the match: a change, which no AI's call makes, or an order its unit
/// takes, which an AI's call gives (06-script-api.md, An AI reads and orders).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Change,
    Order,
}

/// How the next effect in order applies: it takes the first of its type's queue, and applies it.
type ApplyNext = fn(&mut World, &mut Frame);

/// The effects a call queued: one queue for each effect type, made at its first effect and kept
/// for the match, and the apply of each effect in the order queued. A type's effects apply in
/// their own queue's order, so the order list needs no index.
#[derive(Debug, Default)]
pub(crate) struct Effects {
    queues: TypedList<dyn Queue>,
    order: Vec<ApplyNext>,
}

/// One effect type's queue, as `Effects` holds every type.
trait Queue: TypedItem + Debug {
    fn clear(&mut self);
}

impl<E: Effect> Queue for VecDeque<E> {
    fn clear(&mut self) {
        Self::clear(self);
    }
}

impl Effects {
    /// Queues `effect` after every effect queued before it.
    pub(crate) fn push<E: Effect>(&mut self, effect: E) {
        let at = self
            .queues
            .index_of::<VecDeque<E>>()
            .unwrap_or_else(|| self.queues.add(Box::new(VecDeque::<E>::new())));
        self.queues.typed_mut::<VecDeque<E>>(at).push_back(effect);
        self.order.push(Self::apply_first::<E>);
    }

    /// The effects of type `E` queued and not applied yet, first first.
    pub(crate) fn queued<E: Effect>(&self) -> impl Iterator<Item = &E> {
        self.queues.get::<VecDeque<E>>().into_iter().flatten()
    }

    /// The effects of type `E` queued and not applied yet, first first, to change: a later call
    /// may add to an effect it finds, as a grant to a unit the call spawns joins the spawn.
    pub(crate) fn queued_mut<E: Effect>(&mut self) -> impl Iterator<Item = &mut E> {
        self.queues.get_mut::<VecDeque<E>>().into_iter().flatten()
    }

    /// The apply of each effect, in the order queued.
    pub(crate) fn order(&self) -> &[ApplyNext] {
        &self.order
    }

    pub(crate) fn clear(&mut self) {
        self.order.clear();
        for queue in self.queues.iter_mut() {
            queue.clear();
        }
    }

    /// Takes the first effect of type `E` the call in `frame` queued, which the order names
    /// next, and applies it.
    fn apply_first<E: Effect>(world: &mut World, frame: &mut Frame) {
        let effect = frame
            .effects
            .queues
            .get_mut::<VecDeque<E>>()
            .and_then(VecDeque::pop_front)
            .expect("the effect the order names is queued");
        effect.apply(world, frame);
    }
}

#[cfg(test)]
mod tests;
