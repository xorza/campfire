use crate::interpolation_registry::{self, InterpolationRegistry};
use bevy_app::{App, Plugin};

/// Holds the interpolation registry, which a shared protocol registers into on both sides, and
/// finalizes it once the plugins are built.
#[derive(Debug, Default)]
pub struct InterpolationRegistryPlugin;

impl Plugin for InterpolationRegistryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InterpolationRegistry>();
    }

    fn finish(&self, app: &mut App) {
        interpolation_registry::finish(app);
    }
}
