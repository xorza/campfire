use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::book::Book;
use crate::production::mode_supply_data::ModeSupplyData;
use crate::production::production_column::ProductionColumn;
use crate::scripts::frame::Frame;
use crate::units::view::View;

/// The mode's `[supply]`; none in a mode that counts no supply. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SupplyBook(pub(crate) Option<ModeSupplyData>);

impl Book for SupplyBook {
    const OWNER: Option<Capability> = Some(Capability::Production);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        ProductionColumn::share_rules(view, self.0);
    }
}
