use std::sync::Arc;

use bevy::ecs::resource::Resource;
use campfire_package::{ModePackages, Texts};

/// The mode's packages the client plays, and their text in the mode's own language: the one
/// resource every part of the client reads them from.
#[derive(Resource, Debug, Clone)]
pub(crate) struct ClientPackages {
    pub(crate) packages: Arc<ModePackages>,
    pub(crate) texts: Arc<Texts>,
}

impl ClientPackages {
    /// The packages `packages`, with their own text, with no translation.
    pub(crate) fn of(packages: &Arc<ModePackages>) -> Self {
        let texts = Texts::new(packages, &[]).expect("a mode's own text, with no translation");
        Self {
            packages: Arc::clone(packages),
            texts: Arc::new(texts),
        }
    }
}
