use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_script::View;
use campfire_sim::Capability;

use crate::slot_kinds::SlotKinds;

/// The mode's kinds of slot, which the `learn` order reads; none before a mode declares them.
/// Package data, not state. A clone shares the kinds, as the script view reads them.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotKindBook(pub Arc<SlotKinds>);

impl Book for SlotKindBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    /// Names the kinds to scripts; the actions column takes the book from the match at each read.
    fn share(&self, view: &View) {
        view.names_mut().set_table(Arc::clone(self.0.names()));
    }
}
