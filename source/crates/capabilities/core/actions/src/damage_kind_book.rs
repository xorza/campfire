use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::Book;
use campfire_script::MatchNames;
use campfire_values::{DamageKind, Names};

/// The names of the mode's damage kinds, by kind, which the module that deals damage declares and
/// an action's data and the script view name them by.
#[derive(Resource, Debug, Clone)]
pub struct DamageKindBook(pub Arc<Names<DamageKind>>);

impl Book for DamageKindBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(&self.0));
    }
}
