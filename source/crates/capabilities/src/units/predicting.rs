use bevy_ecs::resource::Resource;

/// Marks a client's world, which predicts its player's units: it starts their actions as the
/// server does, and runs none of their effects, which come from the server.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct Predicting;
