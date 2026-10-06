use bevy_ecs::resource::Resource;
use campfire_store::AppendWatch;

/// The running session's journal, as the server watches it: how many records are durable, and
/// whether a write or a sync failed. Present once the match starts with a journal.
#[derive(Resource, Debug, Clone)]
pub struct JournalWatch(pub AppendWatch);
