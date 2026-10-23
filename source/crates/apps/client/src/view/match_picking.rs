use bevy::ecs::resource::Resource;
use campfire_client_files::PickingFile;

/// The mode's own package's picking file, its default for a package of none, which picking
/// resolves against each match's tags as they come.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub(crate) struct MatchPicking(pub(crate) PickingFile);
