use bevy_ecs::resource::Resource;
use campfire_visibility::Visibility;

/// The visibility of the server's session, as its terms carry it (14-replication-audiences.md,
/// Three layers): each kind's filter takes its kind's audience from it as it is inserted.
#[derive(Resource, Debug, Clone, Copy)]
pub(crate) struct MatchVisibility(pub(crate) Visibility);
