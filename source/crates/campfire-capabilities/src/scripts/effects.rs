use std::any::Any;
use std::collections::VecDeque;
use std::fmt::Debug;

use bevy_ecs::world::World;
use campfire_common::Tick;

use crate::scripts::frame::Frame;

/// An effect a call queues: it applies itself when the call returns, in the order queued among
/// every type's.
pub(crate) trait Effect: Debug + 'static {
    /// Applies the effect, which the call in `frame` queued, in tick `now`.
    fn apply(self, world: &mut World, frame: &mut Frame, now: Tick);
}

/// How the next effect in order applies: it takes the first of its type's queue, and applies it.
type ApplyNext = fn(&mut World, &mut Frame, Tick);

/// The effects a call queued: one queue for each effect type, made at its first effect and kept
/// for the match, and the apply of each effect in the order queued. A type's effects apply in
/// their own queue's order, so the order list needs no index.
#[derive(Debug, Default)]
pub(crate) struct Effects {
    queues: Vec<Box<dyn Queue>>,
    order: Vec<ApplyNext>,
}

/// One effect type's queue, as `Effects` holds every type.
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
        let at = self
            .queues
            .iter()
            .position(|queue| queue.as_any().is::<VecDeque<E>>())
            .unwrap_or_else(|| {
                self.queues.push(Box::new(VecDeque::<E>::new()));
                self.queues.len() - 1
            });
        self.queues[at]
            .as_any_mut()
            .downcast_mut::<VecDeque<E>>()
            .expect("the queue of its type")
            .push_back(effect);
        self.order.push(Effects::apply_first::<E>);
    }

    /// The effects of type `E` queued and not applied yet, first first.
    pub(crate) fn queued<E: Effect>(&self) -> impl Iterator<Item = &E> {
        let queue = self
            .queues
            .iter()
            .find_map(|queue| queue.as_any().downcast_ref::<VecDeque<E>>());
        queue.into_iter().flatten()
    }

    /// The apply of each effect, in the order queued.
    pub(crate) fn order(&self) -> &[ApplyNext] {
        &self.order
    }

    pub(crate) fn clear(&mut self) {
        self.order.clear();
        for queue in &mut self.queues {
            queue.clear();
        }
    }

    /// Takes the first effect of type `E` the call in `frame` queued, which the order names
    /// next, and applies it.
    fn apply_first<E: Effect>(world: &mut World, frame: &mut Frame, now: Tick) {
        let effect = frame
            .effects
            .queues
            .iter_mut()
            .find_map(|queue| queue.as_any_mut().downcast_mut::<VecDeque<E>>())
            .and_then(VecDeque::pop_front)
            .expect("the effect the order names is queued");
        effect.apply(world, frame, now);
    }
}

#[cfg(test)]
mod tests {
    use bevy_ecs::resource::Resource;

    use super::*;

    /// What the test's effects applied, in order.
    #[derive(Resource, Debug, Default)]
    struct Applied(Vec<String>);

    #[derive(Debug, PartialEq, Eq)]
    struct Hit(u8);

    impl Effect for Hit {
        fn apply(self, world: &mut World, _: &mut Frame, now: Tick) {
            world
                .resource_mut::<Applied>()
                .0
                .push(format!("hit {} at {now}", self.0));
        }
    }

    /// A second type, as one capability may queue several.
    #[derive(Debug, PartialEq, Eq)]
    struct Heal(u8);

    impl Effect for Heal {
        fn apply(self, world: &mut World, _: &mut Frame, now: Tick) {
            world
                .resource_mut::<Applied>()
                .0
                .push(format!("heal {} at {now}", self.0));
        }
    }

    #[test]
    fn effects_apply_themselves_in_their_order_across_types_and_their_own_within_one() {
        let mut world = World::new();
        world.init_resource::<Applied>();
        let mut frame = Frame::default();
        frame.effects.push(Hit(1));
        frame.effects.push(Heal(2));
        frame.effects.push(Hit(3));
        let hits: Vec<_> = frame.effects.queued::<Hit>().collect();
        assert_eq!(hits, [&Hit(1), &Hit(3)]);
        assert_eq!(frame.effects.queued::<Heal>().count(), 1);
        frame.apply(&mut world, Tick::new(7));
        assert_eq!(
            world.resource::<Applied>().0,
            ["hit 1 at 7", "heal 2 at 7", "hit 3 at 7"]
        );
        // Applied, the frame holds no effect; a type never queued has none, and a queue made
        // by an earlier call serves the next.
        assert_eq!(frame.effects.order().len(), 0);
        assert_eq!(frame.effects.queued::<Hit>().count(), 0);
        let mut fresh = Effects::default();
        assert_eq!(fresh.queued::<Heal>().count(), 0);
        frame.effects.push(Heal(4));
        frame.apply(&mut world, Tick::new(8));
        assert_eq!(world.resource::<Applied>().0.last().unwrap(), "heal 4 at 8");
        fresh.push(Hit(5));
        fresh.clear();
        assert_eq!(fresh.queued::<Hit>().count(), 0);
    }
}
