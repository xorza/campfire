use bevy_ecs::resource::Resource;
use campfire_visibility::OutView;

use crate::capability_set::CapabilitySet;
use crate::role::Role;

/// What a match is, fixed as it is built: the capabilities it declares, its role, and what a
/// player out of play sees in it.
#[derive(Resource, Debug, Clone)]
pub struct MatchFacts {
    pub set: CapabilitySet,
    pub role: Role,
    pub out: OutView,
}
