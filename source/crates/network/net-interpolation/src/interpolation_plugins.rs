use bevy_app::{PluginGroup, PluginGroupBuilder};

use crate::interpolation_plugin::InterpolationPlugin;
use crate::interpolation_timeline_plugin::InterpolationTimelinePlugin;

/// A client's delayed interpolation of what it receives, on the shared
/// [`InterpolationRegistryPlugin`](crate::InterpolationRegistryPlugin): the interpolation
/// timeline, the sets, and the stages each rule installs its systems in.
#[derive(Debug)]
pub struct InterpolationPlugins;

impl PluginGroup for InterpolationPlugins {
    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::start::<Self>()
            .add(InterpolationTimelinePlugin)
            .add(InterpolationPlugin)
    }
}
