use bevy_ecs::resource::Resource;
use bevy_ecs::system::Res;
use campfire_declare::Book;
use campfire_script::View;

use crate::mode_supply_data::ModeSupplyData;

/// The mode's `[supply]`; none in a mode that counts no supply. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupplyBook(pub Option<ModeSupplyData>);

impl SupplyBook {
    /// The run condition of what counts supply: whether the mode counts it.
    pub(crate) fn counts(book: Res<'_, Self>) -> bool {
        book.0.is_some()
    }
}

impl Book for SupplyBook {
    const SCRIPTS_ONLY: bool = false;

    /// The production column takes it from the match at each read.
    fn share(&self, _: &View) {}
}
