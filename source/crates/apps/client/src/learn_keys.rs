use bevy::app::{App, Plugin, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use campfire_actions::{ActionSlots, SlotKindBook};
use campfire_capabilities::Order;
use campfire_net::PendingOrders;
use campfire_progression::ProgressionDirective;

use crate::input_map::{ChordRow, InputMap};
use crate::orders::{OrderSystems, OrdersPlugin};
use crate::own_player::OwnPlayer;

/// The key map's learn chords learn the next rank of the avatar's abilities, the slots of the
/// kinds whose ranks the player learns, in order, as the cast chords cast them.
#[derive(Debug)]
pub(crate) struct LearnKeysPlugin;

impl Plugin for LearnKeysPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            Self::learn.run_if(Self::learn_pressed).in_set(OrderSystems),
        );
    }
}

impl LearnKeysPlugin {
    fn learn(
        (keys, input): (Res<'_, ButtonInput<KeyCode>>, Res<'_, InputMap>),
        own: OwnPlayer<'_, '_>,
        (kinds, avatars): (Res<'_, SlotKindBook>, Query<'_, '_, &ActionSlots>),
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(avatar) = own.avatar() else {
            return;
        };
        let Ok(slots) = avatars.get(avatar.entity) else {
            return;
        };
        let learned = OrdersPlugin::abilities(slots, &kinds.0).zip(input.row(ChordRow::Learn));
        for (slot, &chord) in learned {
            if InputMap::chord_pressed(chord, &keys) {
                let directive = ProgressionDirective::Learn { slot };
                orders.push(Order::one(avatar.id, directive));
            }
        }
    }

    /// Whether a learn chord went down this frame.
    fn learn_pressed(keys: Res<'_, ButtonInput<KeyCode>>, input: Res<'_, InputMap>) -> bool {
        input
            .row(ChordRow::Learn)
            .iter()
            .any(|&chord| InputMap::chord_pressed(chord, &keys))
    }
}
