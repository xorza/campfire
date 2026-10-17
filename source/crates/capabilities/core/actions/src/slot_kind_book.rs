use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::{Book, BookCx, BookError};
use campfire_script::View;

use crate::slot_kinds::SlotKinds;

/// The mode's kinds of slot, which the `learn` order reads; none before a mode declares them.
/// Package data, not state. A clone shares the kinds, as the script view reads them.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotKindBook(pub Arc<SlotKinds>);

impl SlotKindBook {
    /// The mode's `[[slots]]`, none when it gives none.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let kinds = cx.mode_section::<SlotKinds>().cloned();
        Ok(Self(Arc::new(kinds.unwrap_or_default())))
    }
}

impl Book for SlotKindBook {
    const SCRIPTS_ONLY: bool = false;

    /// Names the kinds to scripts; the actions column takes the book from the match at each read.
    fn share(&self, view: &View) {
        view.names_mut().set_table(Arc::clone(self.0.names()));
    }
}
