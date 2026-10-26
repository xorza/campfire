use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::event::Event;
use bevy::ecs::query::{Allow, Changed, With};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Commands, Query, Res, ResMut, SystemParam};
use campfire_actions::{ActionSlots, SlotTimes};
use campfire_common::Tick;
use campfire_host::TickRun;
use campfire_sim::{SimTick, Unpredicted};
use campfire_values::ActionId;

use crate::view::drawing::Drawing;

/// A shot the client saw a unit fire: the unit, the place of the slot that fired among its slots,
/// the slot's action, and the sim's tick after the tick it fired in.
#[derive(Event, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShotSeen {
    pub(crate) unit: Entity,
    pub(crate) slot: usize,
    pub(crate) action: ActionId,
    pub(crate) tick: Tick,
}

/// The shots the client saw since the effects last played them, in the order it saw them.
#[derive(Resource, Debug, Default)]
pub(crate) struct SeenShots(pub(crate) Vec<ShotSeen>);

/// On a unit's drawing's root: the latest tick the client saw each slot of its unit ready at, a
/// rise past which is a shot (02-engine-core.md, The client derives an attack's phase), a
/// rollback that sets a slot back and fires it again playing no second shot; the barrel of its
/// look each weapon slot of its look fires its next shot from (effects.md, A shot's effect at its
/// barrel); and the barrel of its type each of its unit's slots launches its next projectile from
/// (02-engine-core.md, A predicted shot draws a stand-in).
#[derive(Component, Debug, Default)]
pub(crate) struct FireSeen {
    ready: Vec<Tick>,
    pub(crate) turns: SlotTurns,
    pub(crate) launches: SlotTurns,
}

/// The barrel each slot fires its next shot from, by the slot's place, as Zero Hour turns a
/// weapon's barrel with each shot.
#[derive(Debug, Default)]
pub(crate) struct SlotTurns(Vec<usize>);

/// What a sight of a unit's slots' times does with a rise: plays it as a shot at the tick, or holds
/// the ready ticks alone, in a tick a rollback replays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sight {
    Plays(Tick),
    Holds,
}

/// What a sight of shots writes: the roots' sights, the shots seen, and a first sight's insert.
#[derive(SystemParam)]
pub(crate) struct Sights<'w, 's> {
    seen: Query<'w, 's, &'static mut FireSeen>,
    shots: ResMut<'w, SeenShots>,
    commands: Commands<'w, 's>,
}

/// A unit as its slots' times are seen: the unit, its times and its slots.
#[derive(Debug, Clone, Copy)]
struct TimedUnit<'a> {
    unit: Entity,
    times: &'a SlotTimes,
    slots: &'a ActionSlots,
}

/// A unit whose slots' times changed: the unit, its times, its slots and its drawing.
type Timed = (
    Entity,
    &'static SlotTimes,
    &'static ActionSlots,
    &'static Drawing,
);

/// The units the client predicts whose slots' times changed in the tick.
type Predicted<'w, 's> = Query<'w, 's, Timed, Changed<SlotTimes>>;

/// The units the client only receives whose slots' times changed in the frame.
type Received<'w, 's> =
    Query<'w, 's, Timed, (Changed<SlotTimes>, With<Unpredicted>, Allow<Unpredicted>)>;

impl FireSeen {
    /// Sees the shots of the units the client predicts, in each tick it runs: a rise of a slot's
    /// ready tick is a shot in a tick the client predicts the first time, and a tick a rollback
    /// replays updates the ready ticks it holds alone, so a shot the replay moves to a later tick
    /// plays once (02-engine-core.md, A shot plays in its tick's first prediction).
    pub(crate) fn see_predicted(
        (run, tick): (Res<'_, TickRun>, Res<'_, SimTick>),
        units: Predicted<'_, '_>,
        mut sights: Sights<'_, '_>,
    ) {
        let sight = match *run {
            TickRun::First => Sight::Plays(tick.start()),
            TickRun::Replay => Sight::Holds,
        };
        sights.see(units.iter(), sight);
    }

    /// Sees the shots of the units the client only receives, each frame, which no rollback
    /// replays.
    pub(crate) fn see_received(
        tick: Res<'_, SimTick>,
        units: Received<'_, '_>,
        mut sights: Sights<'_, '_>,
    ) {
        sights.see(units.iter(), Sight::Plays(tick.start()));
    }

    /// A unit's first sight, at `times`, which holds no shot.
    fn new(times: &SlotTimes) -> Self {
        Self {
            ready: times.iter().map(|time| time.ready_at).collect(),
            ..Self::default()
        }
    }

    /// Takes the slots' times of `timed`: each slot whose ready tick rose past the one it holds
    /// fired, a shot it adds to `shots` as `sight` plays it, none when its slots changed in
    /// number; then it holds the latest of each.
    fn see(&mut self, timed: TimedUnit<'_>, sight: Sight, shots: &mut SeenShots) {
        let TimedUnit { unit, times, slots } = timed;
        let fresh = self.ready.len() != times.iter().len();
        if let (Sight::Plays(tick), false) = (sight, fresh) {
            for (at, (time, slot)) in times.iter().zip(slots.iter()).enumerate() {
                let rose = self.ready.get(at).is_some_and(|held| time.ready_at > *held);
                if let Some(action) = slot.action.filter(|_| rose) {
                    shots.0.push(ShotSeen {
                        unit,
                        slot: at,
                        action,
                        tick,
                    });
                }
            }
        }
        if fresh {
            self.ready.clear();
            self.ready.extend(times.iter().map(|time| time.ready_at));
        } else {
            for (held, time) in self.ready.iter_mut().zip(times.iter()) {
                *held = (*held).max(time.ready_at);
            }
        }
    }
}

impl Sights<'_, '_> {
    /// Sees each of `units` as `sight` takes its shots; a unit's first sight holds its times.
    fn see<'a>(
        &mut self,
        units: impl Iterator<Item = (Entity, &'a SlotTimes, &'a ActionSlots, &'a Drawing)>,
        sight: Sight,
    ) {
        for (unit, times, slots, drawing) in units {
            let root = drawing.root();
            if let Ok(mut fired) = self.seen.get_mut(root) {
                let timed = TimedUnit { unit, times, slots };
                fired.see(timed, sight, &mut self.shots);
            } else {
                self.commands.entity(root).insert(FireSeen::new(times));
            }
        }
    }
}

impl SlotTurns {
    /// The barrel of `barrels` the slot `slot` fires from, and turns it to the next, back to the
    /// first after the last.
    pub(crate) fn turn(&mut self, slot: usize, barrels: usize) -> usize {
        if self.0.len() <= slot {
            self.0.resize(slot + 1, 0);
        }
        let barrel = self.0[slot] % barrels;
        self.0[slot] = (barrel + 1) % barrels;
        barrel
    }
}

#[cfg(test)]
mod tests;
