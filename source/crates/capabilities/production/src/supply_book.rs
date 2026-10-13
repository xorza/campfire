use bevy_ecs::resource::Resource;
use campfire_core::{Book, Frame, View};
use campfire_sim::Capability;

use crate::mode_supply_data::ModeSupplyData;
use crate::production_column::ProductionColumn;

/// The mode's `[supply]`; none in a mode that counts no supply. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupplyBook(pub Option<ModeSupplyData>);

impl Book for SupplyBook {
    const OWNER: Option<Capability> = Some(Capability::Production);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        ProductionColumn::share_rules(view, self.0);
    }
}
