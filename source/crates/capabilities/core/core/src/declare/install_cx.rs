use std::any::TypeId;
use std::fmt;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_sim::Capability;

use crate::declare::callbacks::Callbacks;
use crate::declare::capability_set::CapabilitySet;

/// What a module's install reads and changes: the match's world and schedule, the capabilities
/// the match declares, whether it runs scripts, whether a client predicts it, and the registries
/// of callbacks its modules call or declare callbacks in.
pub struct InstallCx<'a> {
    pub world: &'a mut World,
    pub schedule: &'a mut Schedule,
    set: CapabilitySet,
    scripts: bool,
    predicting: bool,
    called: Vec<TypeId>,
}

impl<'a> InstallCx<'a> {
    pub const fn new(
        world: &'a mut World,
        schedule: &'a mut Schedule,
        set: CapabilitySet,
        scripts: bool,
        predicting: bool,
        called: Vec<TypeId>,
    ) -> InstallCx<'a> {
        InstallCx {
            world,
            schedule,
            set,
            scripts,
            predicting,
            called,
        }
    }

    /// The capabilities the match declares.
    pub const fn set(&self) -> CapabilitySet {
        self.set
    }

    /// Whether the match declares `capability`.
    pub const fn has(&self, capability: Capability) -> bool {
        self.set.contains(capability)
    }

    /// Whether the match runs scripts: a server's does, a client's predicts with none.
    pub const fn scripts(&self) -> bool {
        self.scripts
    }

    /// Whether a module of the match calls the registry `C`, or declares a callback in it.
    pub fn calls_for<C: Callbacks>(&self) -> bool {
        self.called.contains(&TypeId::of::<C>())
    }

    /// Whether a client predicts the match: it builds no system that computes the server's own
    /// state, which reaches it from the server (design 14, D12).
    pub const fn predicting(&self) -> bool {
        self.predicting
    }
}

impl fmt::Debug for InstallCx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstallCx")
            .field("set", &self.set)
            .field("scripts", &self.scripts)
            .field("predicting", &self.predicting)
            .finish_non_exhaustive()
    }
}
