use bevy_ecs::resource::Resource;
use campfire_visibility::Visibility;

use crate::capability_set::CapabilitySet;
use crate::role::Role;

/// What a match is, fixed as it is built: the capabilities it declares, its role, and the
/// session's visibility, as its terms carry it, the one copy every system and the script view
/// reads.
#[derive(Resource, Debug, Clone)]
pub struct MatchFacts {
    pub set: CapabilitySet,
    pub role: Role,
    pub visibility: Visibility,
}
