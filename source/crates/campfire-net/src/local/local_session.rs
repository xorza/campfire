use bevy_ecs::resource::Resource;

/// On a server: it runs on a thread of its one player's client, which may save and load.
#[derive(Resource, Debug)]
pub(crate) struct LocalSession;
