use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_script::MatchNames;
use campfire_values::Names;

use crate::slot_kind::SlotKind;
use crate::slot_kinds::SlotKinds;

/// The mode's kinds of slot, which the `learn` order reads; none before a mode declares them.
/// Package data, not state. A clone shares the kinds, as the script view reads them.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct SlotKindBook(pub Arc<SlotKinds>);

impl SlotKindBook {
    /// The mode's `[[slots]]`, none when it gives none.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let kinds = cx.mode_section::<SlotKinds>().cloned();
        Ok(Self(Arc::new(kinds.unwrap_or_default())))
    }

    /// The names of the mode's slot kinds, which the load resolves them by.
    pub(crate) fn name_table(cx: &BookCx<'_>) -> Result<Arc<Names<SlotKind>>, BookError> {
        Ok(Arc::clone(cx.book::<Self>()?.0.names()))
    }
}

impl Book for SlotKindBook {
    const SCRIPTS_ONLY: bool = false;

    /// Names the kinds to scripts; the actions column takes the book from the match at each read.
    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(self.0.names()));
    }
}
