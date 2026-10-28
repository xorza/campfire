use bevy::ecs::system::SystemId;
use bevy::ecs::world::World;
use campfire_actions::{SlotCommit, SlotKind};
use campfire_values::Rank;

use super::*;
use crate::view::drawing::DrawingOf;

/// A world of one unit of one weapon slot, its drawing, and the systems that see its shots.
struct Sighting {
    world: World,
    unit: Entity,
    predicted: SystemId,
    received: SystemId,
}

impl Sighting {
    fn new(received: bool) -> Self {
        let mut world = World::new();
        Unpredicted::register(&mut world);
        world.init_resource::<TickRun>();
        world.init_resource::<SeenShots>();
        world.insert_resource(SimTick::new(Tick::new(10)));
        let slots = ActionSlots::new([(ActionId::new(0), SlotKind::new(0), Some(Rank::FIRST))]);
        let times = SlotTimes::ready(&slots);
        let unit = world.spawn((slots, times)).id();
        if received {
            world.entity_mut(unit).insert(Unpredicted);
        }
        world.spawn(DrawingOf(unit));
        let predicted = world.register_system(FireSeen::see_predicted);
        let received = world.register_system(FireSeen::see_received);
        Self {
            world,
            unit,
            predicted,
            received,
        }
    }

    /// Runs a tick `tick` the client runs as `run`, in which the slot is ready at `ready`, then
    /// the frame's sight of received units; the shots seen since the last.
    fn tick(&mut self, run: TickRun, tick: u64, ready: u64) -> Vec<ShotSeen> {
        self.world.insert_resource(run);
        self.world.insert_resource(SimTick::new(Tick::new(tick)));
        self.world.get_mut::<SlotTimes>(self.unit).unwrap().commit(
            SlotCommit {
                slot: 0,
                now: Tick::new(tick),
                ready_at: Tick::new(ready),
                charges: None,
                toggle: None,
                player: None,
            },
            None,
        );
        self.world.run_system(self.predicted).unwrap();
        self.world.run_system(self.received).unwrap();
        std::mem::take(&mut self.world.resource_mut::<SeenShots>().0)
    }

    /// The shot of the slot's weapon at `tick`.
    const fn at(&self, tick: u64) -> ShotSeen {
        ShotSeen {
            unit: self.unit,
            slot: 0,
            action: ActionId::new(0),
            tick: Tick::new(tick),
        }
    }
}

#[test]
fn a_predicted_shot_plays_in_its_ticks_first_prediction_alone() {
    let mut sighting = Sighting::new(false);
    let none = Vec::<ShotSeen>::new();
    // The first sight holds the slot's time; a rise to 20 in tick 11 is a shot, of tick 11.
    assert_eq!(sighting.tick(TickRun::First, 10, 0), none);
    assert_eq!(sighting.tick(TickRun::First, 11, 20), [sighting.at(11)]);
    // A rollback restores the slot to 0 and replays the shot a tick later, ready at 21: the
    // replay holds the rise and plays nothing, nor does the next tick predicted.
    assert_eq!(sighting.tick(TickRun::Replay, 11, 0), none);
    assert_eq!(sighting.tick(TickRun::Replay, 12, 21), none);
    assert_eq!(sighting.tick(TickRun::First, 13, 21), none);
    // A shot first predicted in a replay plays none; one in a tick first predicted plays.
    assert_eq!(sighting.tick(TickRun::Replay, 13, 40), none);
    assert_eq!(sighting.tick(TickRun::First, 14, 40), none);
    assert_eq!(sighting.tick(TickRun::First, 15, 50), [sighting.at(15)]);
}

#[test]
fn a_received_shot_plays_in_each_frame_it_rises() {
    // A unit the client only receives is seen each frame, whatever the tick's run: its rises play,
    // and the predicted sight passes it.
    let mut sighting = Sighting::new(true);
    assert!(sighting.tick(TickRun::First, 10, 0).is_empty());
    assert_eq!(sighting.tick(TickRun::Replay, 11, 20), [sighting.at(11)]);
    assert!(sighting.tick(TickRun::First, 12, 20).is_empty());
}

#[test]
fn a_slot_turns_to_its_next_barrel_with_each_shot_and_back_to_the_first() {
    // Slot 1 of two barrels: 0, 1, 0; slot 0 of three, turned on its own: 0, 1, 2, 0.
    let mut turns = SlotTurns::default();
    let slot_one: Vec<usize> = (0..3).map(|_| turns.turn(1, 2)).collect();
    assert_eq!(slot_one, [0, 1, 0]);
    let slot_zero: Vec<usize> = (0..4).map(|_| turns.turn(0, 3)).collect();
    assert_eq!(slot_zero, [0, 1, 2, 0]);
    // A state of fewer barrels takes the turn within its own.
    assert_eq!(turns.turn(0, 1), 0);
}
