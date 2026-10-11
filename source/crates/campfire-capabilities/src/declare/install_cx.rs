use std::fmt;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_sim::Capability;

use crate::declare::capability_set::CapabilitySet;

/// What a module's install reads and changes: the match's world and schedule, the capabilities
/// the match declares, and whether it runs scripts.
pub(crate) struct InstallCx<'a> {
    pub(crate) world: &'a mut World,
    pub(crate) schedule: &'a mut Schedule,
    set: CapabilitySet,
    scripts: bool,
}

impl<'a> InstallCx<'a> {
    pub(crate) const fn new(
        world: &'a mut World,
        schedule: &'a mut Schedule,
        set: CapabilitySet,
        scripts: bool,
    ) -> InstallCx<'a> {
        InstallCx {
            world,
            schedule,
            set,
            scripts,
        }
    }

    /// The capabilities the match declares.
    pub(crate) const fn set(&self) -> CapabilitySet {
        self.set
    }

    /// Whether the match declares `capability`.
    pub(crate) const fn has(&self, capability: Capability) -> bool {
        self.set.contains(capability)
    }

    /// Whether the match runs scripts: a server's does, a client's predicts with none.
    pub(crate) const fn scripts(&self) -> bool {
        self.scripts
    }
}

impl fmt::Debug for InstallCx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstallCx")
            .field("set", &self.set)
            .field("scripts", &self.scripts)
            .finish_non_exhaustive()
    }
}
