use std::any::TypeId;
use std::fmt;

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::ScheduleSystem;
use campfire_sim::Capability;

use crate::callbacks::Callbacks;
use crate::capability_set::CapabilitySet;
use crate::role::Role;
use crate::server_only::ServerOnly;

/// What a module's install reads and changes: the match's schedule, which it adds its systems to,
/// each by the role it serves, and what it knows of the match. All else a module adds is a
/// declaration, which the installer puts in.
pub struct InstallCx<'a> {
    schedule: &'a mut Schedule,
    facts: &'a MatchFacts<'a>,
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
    pub const fn new(schedule: &'a mut Schedule, facts: &'a MatchFacts<'a>) -> Self {
        InstallCx { schedule, facts }
    }

    /// Whether the match declares `capability`.
    pub const fn has(&self, capability: Capability) -> bool {
        self.facts.set.contains(capability)
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
        if self.facts.role.computes_state() {
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
    /// or one of the rules alone.
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

    /// Adds `server` to a server's match and `client` to a predicting client's: two forms of the
    /// same systems, as a system of a `RoleForm` takes, or one that tests what a team sees by the
    /// server's vision or by the units a client holds, as it holds only those its team sees.
    pub fn by_role<M, N>(
        &mut self,
        server: impl IntoScheduleConfigs<ScheduleSystem, M>,
        client: impl IntoScheduleConfigs<ScheduleSystem, N>,
    ) {
        if self.facts.role.computes_state() {
            self.schedule.add_systems(server);
        } else {
            self.schedule.add_systems(client);
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
