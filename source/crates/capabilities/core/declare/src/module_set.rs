use bevy_ecs::schedule::SystemSet;
use campfire_sim::Capability;

use crate::set_place::SetPlace;

/// A module's system set: the phase or the gap it runs in, stated once. Its order against other
/// sets is its phase's in `Phase::ALL`, so no module names another's sets. A system names one set
/// of its own module, and orders within its set by `.chain()`; one that only some matches build,
/// as one `InstallCx::server` adds, by `.before` or `.after` a system of its set that every match
/// of its module builds.
pub trait ModuleSet: SystemSet + Copy {
    /// The capability of the module that owns it; `None` for the core's.
    const CAPABILITY: Option<Capability>;
    /// Every variant.
    const ALL: &'static [Self];

    /// The phase or the gap it runs in.
    fn place(self) -> SetPlace;
}
