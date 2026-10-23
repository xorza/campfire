use crate::input_delay_setup::InputDelaySetup;
use crate::sync_setup::SyncSetup;
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use campfire_net_core::PositiveTickSpan;

/// The settings shared by local clock synchronization and input scheduling (11-net-stack/time.md,
/// An input leads by one margin for each reason).
///
/// Input delay is part of the clock objective because it affects how far
/// the local simulation must lead the remote simulation.
#[derive(Debug, Resource, Reflect)]
pub struct InputTimelineSetup {
    pub(crate) sync: SyncSetup,
    pub(crate) input_delay_setup: InputDelaySetup,
}

impl InputTimelineSetup {
    /// Maximum number of ticks the local simulation may predict beyond confirmed remote input.
    #[inline]
    pub const fn max_predicted_ticks(&self) -> u16 {
        self.input_delay_setup.max_predicted_ticks()
    }
}

impl Default for InputTimelineSetup {
    /// No tail margin: the objective's pipeline tick already makes an input reach the server
    /// before the server simulates its tick, as Netcode for Entities' 2-tick command slack does
    /// with the sync deadband.
    fn default() -> Self {
        Self {
            sync: SyncSetup {
                jitter_margin: PositiveTickSpan::from_ticks(0),
                ..SyncSetup::default()
            },
            input_delay_setup: InputDelaySetup::no_input_delay(),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::input_delay_setup::InputDelaySetup;
    use crate::input_timeline_setup::InputTimelineSetup;
    use crate::sync_setup::SyncSetup;

    impl InputTimelineSetup {
        #[must_use]
        pub const fn with_input_delay(mut self, input_delay: InputDelaySetup) -> Self {
            self.input_delay_setup = input_delay;
            self
        }

        #[must_use]
        pub const fn with_sync_setup(mut self, sync_setup: SyncSetup) -> Self {
            self.sync = sync_setup;
            self
        }
    }
}
