use bevy_ecs::resource::Resource;

/// A player's link took a slot since the server started: a restored match has none until its
/// players come back.
#[derive(Resource, Debug)]
pub(crate) struct Seated;
