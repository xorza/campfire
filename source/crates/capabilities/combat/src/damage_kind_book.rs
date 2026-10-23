use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::{Book, BookCx, BookError};
use campfire_script::MatchNames;
use campfire_values::{DamageKind, Names};

/// The names of the mode's damage kinds, by kind, as the script view names them.
#[derive(Resource, Debug, Clone)]
#[component(immutable)]
pub(crate) struct DamageKindBook(Arc<Names<DamageKind>>);

impl DamageKindBook {
    /// The names the load resolves the damage kinds by.
    pub(crate) fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        Ok(Self(Arc::clone(cx.names::<DamageKind>()?)))
    }
}

impl Book for DamageKindBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(&self.0));
    }
}
