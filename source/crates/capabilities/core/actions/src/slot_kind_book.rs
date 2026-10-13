use bevy_ecs::resource::Resource;
use campfire_core::{Book, Frame, View};
use campfire_sim::Capability;

use crate::actions_column::ActionsColumn;
use crate::slot_kinds::SlotKinds;

/// The mode's kinds of slot, which the `learn` order reads; none before a mode declares them.
/// Package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotKindBook(pub SlotKinds);

impl Book for SlotKindBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        ActionsColumn::share_kinds(view, self.0.clone());
    }
}
