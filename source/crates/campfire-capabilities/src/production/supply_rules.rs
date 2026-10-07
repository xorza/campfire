use bevy_ecs::resource::Resource;
use serde::Deserialize;

/// The mode's `[supply]`: the mode counts supply, and no player's cap passes `max`. Package data,
/// not state.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyRules {
    pub max: u32,
}
