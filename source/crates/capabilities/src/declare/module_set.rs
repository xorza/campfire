use bevy_ecs::schedule::SystemSet;
use campfire_sim::{Capability, SimPlace};

use crate::declare::set_order::SetOrder;

/// A module's system set: its place in the sim schedule and its order against other modules'
/// sets, stated once. Each method matches every variant, so a new step does not compile until it
/// has both. A system names one set of its own module or of an anchor, and orders within its
/// module by `.chain()` alone.
pub(crate) trait ModuleSet: SystemSet + Copy {
    /// The capability of the module that owns it; `None` for the core's.
    const CAPABILITY: Option<Capability>;
    /// Every variant.
    const ALL: &'static [Self];

    /// The stage or the edge it runs in.
    fn place(self) -> SimPlace;

    /// The sets it runs after and before.
    fn order(self, o: &mut SetOrder);
}
