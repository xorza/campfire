use std::any::TypeId;
use std::fmt;

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::ScheduleSystem;
use bevy_ecs::world::World;
use campfire_sim::Capability;

use crate::declare::callbacks::Callbacks;
use crate::declare::capability_set::CapabilitySet;
use crate::declare::role::Role;
use crate::declare::server_only::ServerOnly;

/// What a module's install reads and changes: the match's world and schedule, and what it knows
/// of the match.
pub struct InstallCx<'a> {
    pub world: &'a mut World,
    pub schedule: &'a mut Schedule,
    facts: MatchFacts<'a>,
}

/// What an install knows of its match: the capabilities it declares, its role, and the registries
/// of callbacks its modules call or declare callbacks in.
#[derive(Debug)]
pub struct MatchFacts<'a> {
    pub set: CapabilitySet,
    pub role: &'a Role,
    pub called: Vec<TypeId>,
}

impl<'a> InstallCx<'a> {
    pub const fn new(
        world: &'a mut World,
        schedule: &'a mut Schedule,
        facts: MatchFacts<'a>,
    ) -> InstallCx<'a> {
        InstallCx {
            world,
            schedule,
            facts,
        }
    }

    /// The capabilities the match declares.
    pub const fn set(&self) -> CapabilitySet {
        self.facts.set
    }

    /// Whether the match declares `capability`.
    pub const fn has(&self, capability: Capability) -> bool {
        self.facts.set.contains(capability)
    }

    /// What the match is to its host.
    pub const fn role(&self) -> &'a Role {
        self.facts.role
    }

    /// Whether a module of the match calls the registry `C`, or declares a callback in it.
    pub fn calls_for<C: Callbacks>(&self) -> bool {
        self.facts.called.contains(&TypeId::of::<C>())
    }

    /// Adds `systems`, which compute the server's own state, to a server's match, in `ServerOnly`
    /// besides their own sets; a client's install leaves them out.
    pub fn server<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
        if let Role::Server(_) = self.facts.role {
            self.schedule.add_systems(systems.in_set(ServerOnly));
        }
    }
}

impl fmt::Debug for InstallCx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstallCx")
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}
