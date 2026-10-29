use core::time::Duration;

use bevy_ecs::prelude::*;
use bevy_time::{Timer, TimerMode};

/// The interval at which the app replicates, which all of its senders share.
#[derive(Debug, Resource)]
pub struct ReplicationTimer {
    pub(crate) timer: Timer,
}

impl ReplicationTimer {
    pub fn new(replication_interval: Duration) -> Self {
        Self {
            timer: Timer::new(replication_interval, TimerMode::Repeating),
        }
    }
}

impl Default for ReplicationTimer {
    fn default() -> Self {
        Self::new(Duration::default())
    }
}
