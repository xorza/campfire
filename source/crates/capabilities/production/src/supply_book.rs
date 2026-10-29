use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_names::MatchNames;

use crate::mode_supply_data::ModeSupplyData;

/// The mode's `[supply]`; none in a mode that counts no supply. Package data, not state.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable)]
pub struct SupplyBook(pub Option<ModeSupplyData>);

impl SupplyBook {
    /// Whether the mode counts supply, which its refresh needs.
    pub(crate) const fn counts(self) -> bool {
        self.0.is_some()
    }
}

impl Book for SupplyBook {
    const SCRIPTS_ONLY: bool = false;

    /// The production column takes it from the match at each read.
    fn share(&self, _: &mut MatchNames) {}
}

impl SupplyBook {
    /// The mode's `[supply]`, none when it counts none.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        Ok(Self(cx.mode_section::<ModeSupplyData>().copied()))
    }
}
