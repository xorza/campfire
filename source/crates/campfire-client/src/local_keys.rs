use std::sync::Arc;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use campfire_net::{Pace, PaceSpeed, PendingSaves, SaveCommand};

/// The keys of a local match: P pauses it and plays it on, 1 to 4 set its speed to 0.5, 1, 2 or
/// 4 times, F5 saves it, and F9 loads its latest save.
#[derive(Debug)]
pub(crate) struct LocalKeys {
    pub(crate) pace: Arc<Pace>,
}

/// The pace the keys set.
#[derive(Resource, Debug)]
struct Keyed(Arc<Pace>);

/// The speed keys, slowest first, as `PaceSpeed::ALL` lists the speeds.
const SPEED_KEYS: [KeyCode; 4] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
];

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
        if keys.just_pressed(KeyCode::KeyP) {
            pace.set_paused(!pace.paused());
        }
        for (key, speed) in SPEED_KEYS.into_iter().zip(PaceSpeed::ALL) {
            if keys.just_pressed(key) {
                pace.set_speed(speed);
            }
        }
        if keys.just_pressed(KeyCode::F5) {
            saves.push(SaveCommand::Save);
        }
        if keys.just_pressed(KeyCode::F9) {
            saves.push(SaveCommand::LoadLatest);
        }
    }
}
