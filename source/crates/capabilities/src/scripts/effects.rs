use std::any::Any;
use std::collections::VecDeque;
use std::fmt::Debug;

use bevy_ecs::world::World;
use campfire_math::Tick;
use campfire_sim::Capability;

use crate::scripts::frame::Frame;

/// An effect a call queues: it applies when the call returns, by the capability it names, in the
/// order queued among every capability's.
pub(crate) trait Effect: Debug + 'static {
    const CAPABILITY: Capability;
}

/// How a capability applies the next of its effects that the call in `frame` queued, in tick
/// `now`: it takes the effect with `Effects::take`.
pub(crate) type ApplyEffect = fn(&mut World, &mut Frame, Tick);

/// The effects a call queued: one queue for each capability's effect type, made at its first
/// effect and kept for the match, and the capability of each effect in the order queued. A
/// capability's effects apply in their own queue's order, so the order list needs no index.
#[derive(Debug, Default)]
pub(crate) struct Effects {
    /// By capability index.
    queues: Vec<Option<Box<dyn Queue>>>,
    order: Vec<Capability>,
}

/// One capability's queue, as `Effects` holds every kind.
trait Queue: Debug {
    fn clear(&mut self);
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<E: Effect> Queue for VecDeque<E> {
    fn clear(&mut self) {
        VecDeque::clear(self);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Effects {
    /// Queues `effect` after every effect queued before it.
    pub(crate) fn push<E: Effect>(&mut self, effect: E) {
        let index = E::CAPABILITY as usize;
        if self.queues.len() <= index {
            self.queues.resize_with(index + 1, || None);
        }
        let queue = self.queues[index].get_or_insert_with(|| Box::new(VecDeque::<E>::new()));
        queue
            .as_any_mut()
            .downcast_mut::<VecDeque<E>>()
            .expect("a capability's queue holds its one effect type")
            .push_back(effect);
        self.order.push(E::CAPABILITY);
    }

    /// The effects of type `E` queued and not applied yet, first first.
    pub(crate) fn queued<E: Effect>(&self) -> impl Iterator<Item = &E> {
        let queue = self
            .queues
            .get(E::CAPABILITY as usize)
            .and_then(Option::as_ref);
        queue
            .map(|queue| {
                queue
                    .as_any()
                    .downcast_ref::<VecDeque<E>>()
                    .expect("a capability's queue holds its one effect type")
            })
            .into_iter()
            .flatten()
    }

    /// Takes the first effect of type `E`, which the apply of the next effect in order takes.
    pub(crate) fn take<E: Effect>(&mut self) -> E {
        self.queues[E::CAPABILITY as usize]
            .as_mut()
            .and_then(|queue| queue.as_any_mut().downcast_mut::<VecDeque<E>>())
            .and_then(VecDeque::pop_front)
            .expect("the effect the order names is queued")
    }

    /// The capability of each effect, in the order queued.
    pub(crate) fn order(&self) -> &[Capability] {
        &self.order
    }

    pub(crate) fn clear(&mut self) {
        self.order.clear();
        for queue in self.queues.iter_mut().flatten() {
            queue.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct Hit(u8);

    impl Effect for Hit {
        const CAPABILITY: Capability = Capability::Combat;
    }

    #[derive(Debug, PartialEq, Eq)]
    struct Spawn(&'static str);

    impl Effect for Spawn {
        const CAPABILITY: Capability = Capability::Mode;
    }

    #[test]
    fn effects_keep_their_order_across_kinds_and_their_own_within_one() {
        let mut effects = Effects::default();
        effects.push(Hit(1));
        effects.push(Spawn("a"));
        effects.push(Hit(2));
        assert_eq!(
            effects.order(),
            [Capability::Combat, Capability::Mode, Capability::Combat]
        );
        let hits: Vec<_> = effects.queued::<Hit>().collect();
        assert_eq!(hits, [&Hit(1), &Hit(2)]);
        assert_eq!(effects.queued::<Spawn>().count(), 1);
        assert_eq!(effects.take::<Hit>(), Hit(1));
        assert_eq!(effects.take::<Spawn>(), Spawn("a"));
        assert_eq!(effects.take::<Hit>(), Hit(2));
        // Cleared, the queues hold nothing, and a kind never queued has none.
        effects.push(Hit(3));
        effects.clear();
        assert_eq!(effects.order(), []);
        assert_eq!(effects.queued::<Hit>().count(), 0);
        let mut fresh = Effects::default();
        assert_eq!(fresh.queued::<Spawn>().count(), 0);
        fresh.push(Spawn("b"));
        assert_eq!(fresh.take::<Spawn>(), Spawn("b"));
    }
}
