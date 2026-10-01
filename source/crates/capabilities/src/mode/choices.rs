use bevy_ecs::resource::Resource;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::mode::offer::Offer;

/// What each player chose: for each player slot, a row of each choice's values, in the order
/// of the mode's choices, `None` where the player has not chosen.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Choices(pub(crate) Vec<Option<Offer>>);

impl SimResource for Choices {
    const NAME: &'static str = "mode.choices";
}
