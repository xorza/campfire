use bevy_reflect::Reflect;
use campfire_net_sync::SyncHysteresis;
use core::time::Duration;

#[derive(Debug, Reflect)]
pub struct InterpolationState {
    pub(crate) remote_send_interval: Duration,
    pub(crate) hysteresis: SyncHysteresis,
    pub(crate) relative_speed: f32,
    pub(crate) is_synced: bool,
}

impl Default for InterpolationState {
    /// Unsynced, at the speed of real time.
    fn default() -> Self {
        Self {
            remote_send_interval: Duration::ZERO,
            hysteresis: SyncHysteresis::default(),
            relative_speed: 1.0,
            is_synced: false,
        }
    }
}
