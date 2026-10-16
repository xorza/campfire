use std::collections::VecDeque;
use std::fmt::Debug;

use bevy_ecs::world::World;

use crate::scripts::frame::Frame;
use crate::scripts::type_slots::{Slot, TypeSlots};

/// An effect a call queues: it applies itself when the call returns, in the order queued among
/// every type's.
pub trait Effect: Debug + 'static {
    /// Applies the effect, which the call in `frame` queued, in the call's tick.
    fn apply(self, world: &mut World, frame: &mut Frame);
}

/// How the next effect in order applies: it takes the first of its type's queue, and applies it.
type ApplyNext = fn(&mut World, &mut Frame);

/// The effects a call queued: one queue for each effect type, made at its first effect and kept
/// for the match, and the apply of each effect in the order queued. A type's effects apply in
/// their own queue's order, so the order list needs no index.
#[derive(Debug, Default)]
pub struct Effects {
    queues: TypeSlots<dyn Queue>,
    order: Vec<ApplyNext>,
}

/// One effect type's queue, as `Effects` holds every type.
trait Queue: Slot + Debug {
    fn clear(&mut self);
}

impl<E: Effect> Queue for VecDeque<E> {
    fn clear(&mut self) {
        Self::clear(self);
    }
}

impl Effects {
    /// Queues `effect` after every effect queued before it.
    pub fn push<E: Effect>(&mut self, effect: E) {
        let at = self
            .queues
            .index_of::<VecDeque<E>>()
            .unwrap_or_else(|| self.queues.add(Box::new(VecDeque::<E>::new())));
        self.queues.typed_mut::<VecDeque<E>>(at).push_back(effect);
        self.order.push(Self::apply_first::<E>);
    }

    /// The effects of type `E` queued and not applied yet, first first.
    pub fn queued<E: Effect>(&self) -> impl Iterator<Item = &E> {
        self.queues.get::<VecDeque<E>>().into_iter().flatten()
    }

    /// The apply of each effect, in the order queued.
    pub fn order(&self) -> &[ApplyNext] {
        &self.order
    }

    pub fn clear(&mut self) {
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
mod tests {
    use bevy_ecs::resource::Resource;
    use campfire_common::Tick;

    use super::*;
    use crate::scripts::call_part::{CallPart, Commit};
    use crate::scripts::call_start::CallStart;
    use crate::scripts::error::CallError;

    /// What the test's effects applied, in order.
    #[derive(Resource, Debug, Default)]
    struct Applied(Vec<String>);

    #[derive(Debug, PartialEq, Eq)]
    struct Hit(u8);

    impl Effect for Hit {
        fn apply(self, world: &mut World, frame: &mut Frame) {
            let now = frame.now();
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
        fn apply(self, world: &mut World, frame: &mut Frame) {
            let now = frame.now();
            world
                .resource_mut::<Applied>()
                .0
                .push(format!("heal {} at {now}", self.0));
        }
    }

    /// A part that commits before the effects, and one that commits after them.
    #[derive(Debug)]
    struct Early;

    #[derive(Debug)]
    struct Late;

    impl CallPart for Early {
        const COMMITS: Commit = Commit::BeforeEffects;

        fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
            Ok(())
        }

        fn commit(&mut self, world: &mut World) {
            world.resource_mut::<Applied>().0.push("early".to_owned());
        }
    }

    impl CallPart for Late {
        const COMMITS: Commit = Commit::AfterEffects;

        fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
            Ok(())
        }

        fn commit(&mut self, world: &mut World) {
            world.resource_mut::<Applied>().0.push("late".to_owned());
        }
    }

    #[test]
    fn effects_apply_themselves_in_their_order_across_types_and_their_own_within_one() {
        let mut world = World::new();
        world.init_resource::<Applied>();
        let mut frame = Frame::in_tick(Tick::new(7));
        // Added in the other order, each part commits at its own side of the effects.
        frame.add_part(Late);
        frame.add_part(Early);
        frame.effects.push(Hit(1));
        frame.effects.push(Heal(2));
        frame.effects.push(Hit(3));
        let hits: Vec<_> = frame.effects.queued::<Hit>().collect();
        assert_eq!(hits, [&Hit(1), &Hit(3)]);
        assert_eq!(frame.effects.queued::<Heal>().count(), 1);
        frame.apply(&mut world);
        assert_eq!(
            world.resource::<Applied>().0,
            ["early", "hit 1 at 7", "heal 2 at 7", "hit 3 at 7", "late"]
        );
        // Applied, the frame holds no effect; a type never queued has none, and a queue made
        // by an earlier call serves the next.
        assert_eq!(frame.effects.order().len(), 0);
        assert_eq!(frame.effects.queued::<Hit>().count(), 0);
        let mut fresh = Effects::default();
        assert_eq!(fresh.queued::<Heal>().count(), 0);
        let mut frame = Frame::in_tick(Tick::new(8));
        frame.effects.push(Heal(4));
        frame.apply(&mut world);
        assert_eq!(world.resource::<Applied>().0.last().unwrap(), "heal 4 at 8");
        fresh.push(Hit(5));
        fresh.clear();
        assert_eq!(fresh.queued::<Hit>().count(), 0);
    }
}
