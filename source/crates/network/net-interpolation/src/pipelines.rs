use bevy_app::App;
use bevy_ecs::resource::Resource;

/// Which pipelines the app runs, whose systems a rule installs: each pipeline's plugin marks its
/// own as it builds.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub(crate) struct Pipelines {
    pub(crate) delayed: bool,
    pub(crate) frame: bool,
}

impl Pipelines {
    pub(crate) fn of(app: &App) -> Self {
        app.world()
            .get_resource::<Self>()
            .copied()
            .unwrap_or_default()
    }

    /// Marks the pipeline `mark` sets as one the app runs.
    pub(crate) fn mark(app: &mut App, mark: impl FnOnce(&mut Self)) {
        mark(&mut app.world_mut().get_resource_or_init::<Self>());
    }
}
