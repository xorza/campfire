use core::time::Duration;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_net_packets::Signal;

/// What an app's frame waits on between its ticks: the IO thread of each of its endpoints raises it
/// as a link has work for the frame, so a runner that waits on it runs the frame then, and not on a
/// fixed loop (13-net-stack/packet-session.md, The IO thread wakes the frame).
#[derive(Resource, Debug, Clone, Default)]
pub struct FrameWake(Arc<Signal>);

impl FrameWake {
    /// Waits until a link has work for the frame, or `timeout` passes.
    pub fn wait(&self, timeout: Duration) {
        self.0.wait(Some(timeout));
    }

    pub(crate) fn signal(&self) -> Arc<Signal> {
        Arc::clone(&self.0)
    }
}
