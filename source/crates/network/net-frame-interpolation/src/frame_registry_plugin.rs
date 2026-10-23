use crate::frame_holders::FrameHolders;
use crate::frame_registry::FrameRegistry;
use bevy_app::{App, Plugin};

/// Holds the frame registry, which a shared protocol registers into on both sides, and builds and
/// seals it once the plugins are built: an app that runs
/// [`FrameInterpolationPlugin`](crate::FrameInterpolationPlugin) installs each rule's systems, now
/// that each component's frame source is known, and one that does not, a server's, installs none.
#[derive(Debug, Default)]
pub struct FrameRegistryPlugin;

impl Plugin for FrameRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameRegistry>();
    }

    fn finish(&self, app: &mut App) {
        let pending = app
            .world_mut()
            .resource_mut::<FrameRegistry>()
            .take_pending();
        if app.world().contains_resource::<FrameHolders>() {
            for rule in pending {
                rule(app);
            }
        }
        app.world_mut().resource_mut::<FrameRegistry>().seal();
    }
}
