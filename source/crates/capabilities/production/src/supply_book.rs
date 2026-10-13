use bevy_ecs::resource::Resource;
use campfire_core::{Book, View};
use campfire_sim::Capability;

use crate::mode_supply_data::ModeSupplyData;

/// The mode's `[supply]`; none in a mode that counts no supply. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupplyBook(pub Option<ModeSupplyData>);

impl Book for SupplyBook {
    const OWNER: Option<Capability> = Some(Capability::Production);
    const SCRIPTS_ONLY: bool = false;

    /// The production column takes it from the match at each read.
    fn share(&self, _: &View) {}
}
