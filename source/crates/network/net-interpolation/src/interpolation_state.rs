use bevy_reflect::Reflect;
use campfire_net_sync::SyncHysteresis;
use core::time::Duration;

#[derive(Default, Debug, Reflect)]
pub struct InterpolationState {
    pub(crate) remote_send_interval: Duration,
    pub(crate) hysteresis: SyncHysteresis,
    pub(crate) relative_speed: f32,
    pub(crate) is_synced: bool,
}
