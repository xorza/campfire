use bevy_ecs::resource::Resource;
use campfire_common::Tick;

use super::*;
use crate::scripts::call_extension::{CallExtension, Commit};
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;

/// What the test's effects applied, in order.
#[derive(Resource, Debug, Default)]
struct Applied(Vec<String>);

#[derive(Debug, PartialEq, Eq)]
struct Hit(u8);

impl Effect for Hit {
    const KIND: EffectKind = EffectKind::Change;

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
    const KIND: EffectKind = EffectKind::Change;

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

impl CallExtension for Early {
    const COMMITS: Commit = Commit::BeforeEffects;

    fn begin_batch(&mut self, _: &World) {}

    fn begin(&mut self, _: &World, _: &CallStart) -> Result<(), CallError> {
        Ok(())
    }

    fn commit(&mut self, world: &mut World) {
        world.resource_mut::<Applied>().0.push("early".to_owned());
    }
}

impl CallExtension for Late {
    const COMMITS: Commit = Commit::AfterEffects;

    fn begin_batch(&mut self, _: &World) {}

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
    frame.add_extension(Late);
    frame.add_extension(Early);
    frame.effects.push(Hit(1));
    frame.effects.push(Heal(2));
    frame.effects.push(Hit(3));
    let hits: Vec<_> = frame.effects.queued::<Hit>().collect();
    assert_eq!(hits, [&Hit(1), &Hit(3)]);
    assert_eq!(frame.effects.queued::<Heal>().count(), 1);
    // A change to a queued effect applies, in its place: the second hit, 3, becomes 30.
    if let Some(hit) = frame.effects.queued_mut::<Hit>().nth(1) {
        hit.0 = 30;
    }
    frame.apply(&mut world);
    assert_eq!(
        world.resource::<Applied>().0,
        ["early", "hit 1 at 7", "heal 2 at 7", "hit 30 at 7", "late"]
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
