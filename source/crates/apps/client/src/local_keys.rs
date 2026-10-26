use std::sync::Arc;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::observer::On;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use campfire_host::{JournalCommand, Pace, PaceSpeed};
use campfire_net::PendingJournalCommands;

use crate::input_map::{Act, InputMap};
use crate::report::ReportTaken;

/// The keys of a local match, by the key map: its pause chord pauses it and plays it on, its
/// slower and faster chords step its speed down and up through 0.5, 1, 2 and 4 times, its save
/// chord saves it, its load chord loads its latest save, and a report the report chord took asks
/// the server for its log so far.
#[derive(Debug)]
pub(crate) struct LocalKeysPlugin {
    pub(crate) pace: Arc<Pace>,
}

/// The pace the keys set.
#[derive(Resource, Debug)]
struct Keyed(Arc<Pace>);

impl Plugin for LocalKeysPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Keyed(Arc::clone(&self.pace)));
        app.add_systems(Update, Self::press);
        app.add_observer(Self::report);
    }
}

impl LocalKeysPlugin {
    fn press(
        (keys, input): (Res<'_, ButtonInput<KeyCode>>, Res<'_, InputMap>),
        keyed: Res<'_, Keyed>,
        mut saves: ResMut<'_, PendingJournalCommands>,
    ) {
        let pace = &keyed.0;
        if input.pressed(Act::Pause, &keys) {
            pace.set_paused(!pace.is_paused());
        }
        let step = i8::from(input.pressed(Act::Faster, &keys))
            - i8::from(input.pressed(Act::Slower, &keys));
        if step != 0 {
            pace.set_speed(Self::stepped(pace.speed(), step));
        }
        if input.pressed(Act::Save, &keys) {
            saves.push(JournalCommand::Save);
        }
        if input.pressed(Act::Load, &keys) {
            saves.push(JournalCommand::LoadLatest);
        }
    }

    /// Asks the server to write its log into the report `taken`'s folder once the log reaches
    /// the report's tick.
    fn report(taken: On<'_, '_, ReportTaken>, mut saves: ResMut<'_, PendingJournalCommands>) {
        saves.push(JournalCommand::Report(taken.key));
    }

    /// The speed `step` places from `speed` in `PaceSpeed::ALL`, held at its ends.
    fn stepped(speed: PaceSpeed, step: i8) -> PaceSpeed {
        let at = PaceSpeed::ALL
            .iter()
            .position(|&listed| listed == speed)
            .expect("every speed is listed");
        let last = PaceSpeed::ALL.len() - 1;
        let to = if step < 0 {
            at.saturating_sub(usize::from(step.unsigned_abs()))
        } else {
            (at + usize::from(step.unsigned_abs())).min(last)
        };
        PaceSpeed::ALL[to]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_moves_the_speed_one_place_and_stops_at_its_ends() {
        let [half, one, two, four] = PaceSpeed::ALL;
        assert_eq!(LocalKeysPlugin::stepped(one, 1), two);
        assert_eq!(LocalKeysPlugin::stepped(one, -1), half);
        assert_eq!(LocalKeysPlugin::stepped(half, -1), half);
        assert_eq!(LocalKeysPlugin::stepped(four, 1), four);
    }
}
