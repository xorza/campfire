use bevy::ecs::resource::Resource;
use campfire_client_files::HudFile;

/// The mode's own package's HUD file, its default for a package of none, as the match's frames
/// read it.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MatchHud(pub(crate) HudFile);
