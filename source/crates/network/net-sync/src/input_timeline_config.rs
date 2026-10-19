use crate::input_delay_config::InputDelayConfig;
use crate::sync_config::SyncConfig;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_core::PositiveTickDelta;

/// Configuration shared by local clock synchronization and input scheduling.
///
/// Input delay is part of the clock objective because it affects how far
/// the local simulation must lead the remote simulation.
#[derive(Debug, Resource, Reflect)]
pub struct InputTimelineConfig {
    pub(crate) sync: SyncConfig,
    pub(crate) input_delay_config: InputDelayConfig,
}

impl InputTimelineConfig {
    pub const fn new(sync_config: SyncConfig, input_delay: InputDelayConfig) -> Self {
        Self {
            sync: sync_config,
            input_delay_config: input_delay,
        }
    }

    #[must_use]
    pub const fn with_input_delay(mut self, input_delay: InputDelayConfig) -> Self {
        self.input_delay_config = input_delay;
        self
    }

    #[must_use]
    pub const fn with_sync_config(mut self, sync_config: SyncConfig) -> Self {
        self.sync = sync_config;
        self
    }

    /// Maximum number of ticks the local simulation may predict beyond confirmed remote input.
    #[inline]
    pub const fn maximum_predicted_ticks(&self) -> u16 {
        self.input_delay_config.maximum_predicted_ticks()
    }
}

impl Default for InputTimelineConfig {
    /// No tail margin: the objective's pipeline tick already makes an input reach the server
    /// before the server simulates its tick, as Netcode for Entities' 2-tick command slack does
    /// with the sync deadband.
    fn default() -> Self {
        Self {
            sync: SyncConfig {
                jitter_margin: PositiveTickDelta::lit("0"),
                ..SyncConfig::default()
            },
            input_delay_config: InputDelayConfig::no_input_delay(),
        }
    }
}
