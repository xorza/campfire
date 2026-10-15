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

    /// Whether the match runs scripts, for the state an install builds that the scripts call
    /// into; its systems name their role by the method that adds them.
    pub const fn scripts(&self) -> bool {
        self.facts.role.scripts()
    }

    /// Whether a module of the match calls the registry `C`, or declares a callback in it.
    pub fn calls_for<C: Callbacks>(&self) -> bool {
        self.facts.called.contains(&TypeId::of::<C>())
    }

    /// Adds `systems` to every match, a server's and a client's alike.
    pub fn systems<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
        self.schedule.add_systems(systems);
    }

    /// Adds `systems`, which compute the server's own state, to a server's match, in `ServerOnly`
    /// besides their own sets; a client's install leaves them out.
    pub fn server<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
        if let Role::Server(_) = self.facts.role {
            self.schedule.add_systems(systems.in_set(ServerOnly));
        }
    }

    /// Adds `systems`, which call scripts, to a match that runs them, a server's within its
    /// budgets.
    pub fn scripted<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
        if self.facts.role.scripts() {
            self.schedule.add_systems(systems);
        }
    }

    /// Adds `systems`, which stand in for what scripts do, to a match that runs none: a client's,
    /// or a server's of no budgets, as a test of the rules alone builds.
    pub fn unscripted<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
        if !self.facts.role.scripts() {
            self.schedule.add_systems(systems);
        }
    }

    /// Adds `systems` to a predicting client's match alone: what it predicts of what the server
    /// computes.
    pub fn predicting<M>(&mut self, systems: impl IntoScheduleConfigs<ScheduleSystem, M>) {
        if matches!(self.facts.role, Role::Client) {
            self.schedule.add_systems(systems);
        }
    }

    /// Adds `vision` to a server's match and `holding` to a client's: two forms of the same
    /// systems, which test what a team sees by the server's vision, or by the units a client
    /// holds, as it holds only those its team sees.
    pub fn sighted<M, N>(
        &mut self,
        vision: impl IntoScheduleConfigs<ScheduleSystem, M>,
        holding: impl IntoScheduleConfigs<ScheduleSystem, N>,
    ) {
        match self.facts.role {
            Role::Server(_) => self.schedule.add_systems(vision),
            Role::Client => self.schedule.add_systems(holding),
        };
    }
}

impl fmt::Debug for InstallCx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstallCx")
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}
