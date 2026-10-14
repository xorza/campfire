use std::sync::Arc;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use campfire_net::{Pace, PaceSpeed, PendingSaves, SaveCommand};

/// The keys of a local match: Space pauses it and plays it on, minus and equals, or the keypad's
/// minus and plus, step its speed down and up through 0.5, 1, 2 and 4 times, F5 saves it, and F9
/// loads its latest save. P and the digits are the shop's and the items' (design 13, D12).
#[derive(Debug)]
pub(crate) struct LocalKeys {
    pub(crate) pace: Arc<Pace>,
}

/// The pace the keys set.
#[derive(Resource, Debug)]
struct Keyed(Arc<Pace>);

/// The keys that step the speed down, and up.
const SLOWER_KEYS: [KeyCode; 2] = [KeyCode::Minus, KeyCode::NumpadSubtract];
const FASTER_KEYS: [KeyCode; 2] = [KeyCode::Equal, KeyCode::NumpadAdd];

impl Plugin for LocalKeys {
    fn build(&self, app: &mut App) {
        app.insert_resource(Keyed(Arc::clone(&self.pace)));
        app.add_systems(Update, LocalKeys::press);
    }
}

impl LocalKeys {
    fn press(
        keys: Res<'_, ButtonInput<KeyCode>>,
        keyed: Res<'_, Keyed>,
        mut saves: ResMut<'_, PendingSaves>,
    ) {
        let pace = &keyed.0;
        if keys.just_pressed(KeyCode::Space) {
            pace.set_paused(!pace.paused());
        }
        let step = i8::from(keys.any_just_pressed(FASTER_KEYS))
            - i8::from(keys.any_just_pressed(SLOWER_KEYS));
        if step != 0 {
            pace.set_speed(LocalKeys::stepped(pace.speed(), step));
        }
        if keys.just_pressed(KeyCode::F5) {
            saves.push(SaveCommand::Save);
        }
        if keys.just_pressed(KeyCode::F9) {
            saves.push(SaveCommand::LoadLatest);
        }
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
        assert_eq!(LocalKeys::stepped(one, 1), two);
        assert_eq!(LocalKeys::stepped(one, -1), half);
        assert_eq!(LocalKeys::stepped(half, -1), half);
        assert_eq!(LocalKeys::stepped(four, 1), four);
    }
}
