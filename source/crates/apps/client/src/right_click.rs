use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::SystemSet;
use bevy::math::Vec3;

use crate::pointer::Pointed;

/// The right click of this frame on the ground that no click module took yet: the unit under the
/// cursor, and the ground point under it. Each module that orders by a click takes it as it
/// orders, in the order of `ClickSystems`, so one click gives one order (02-engine-core.md, A
/// client module list).
#[derive(Resource, Debug, Default)]
pub(crate) struct RightClick(Option<ClickAt>);

/// Where a right click is: the unit under the cursor, and the ground point under it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClickAt {
    pub(crate) under: Option<Pointed>,
    pub(crate) point: Vec3,
}

/// The click modules' systems, in the order they look at a right click: its read, then a
/// gather's, a use's and a rally's, which a mode's capabilities add, then the walk or the attack
/// every mode gives.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ClickSystems {
    Read,
    Gather,
    Use,
    Rally,
    Walk,
}

impl RightClick {
    /// The click of this frame, `at`, or none.
    pub(crate) const fn set(&mut self, at: Option<ClickAt>) {
        self.0 = at;
    }

    /// The click no module took yet.
    pub(crate) const fn get(&self) -> Option<ClickAt> {
        self.0
    }

    /// Takes the click, which no module after this one orders by.
    pub(crate) const fn take(&mut self) {
        self.0 = None;
    }
}
