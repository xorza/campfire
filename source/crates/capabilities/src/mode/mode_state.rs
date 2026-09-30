use bevy_ecs::resource::Resource;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::units::state_value::StateValue;

/// The mode's script state: a value for each field its data declares, in the order of their
/// names.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModeState(pub(crate) Vec<StateValue>);

impl ModeState {
    pub const fn get(&self) -> &[StateValue] {
        self.0.as_slice()
    }
}

impl SimResource for ModeState {
    const NAME: &'static str = "mode.state";
}
