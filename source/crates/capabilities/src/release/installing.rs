use std::fmt;

use bevy_ecs::schedule::Schedule;
use bevy_ecs::world::World;
use campfire_declare::{InstallCx, MatchFacts, Module, ModuleVisitor, Role};
use campfire_sim::StateRegistry;

use crate::release::declaring::Declaring;
use crate::release::installer::Installer;

/// The walk that installs each module a match runs, of `role`: its declarations, then its
/// systems.
pub(super) struct Installing<'a> {
    world: &'a mut World,
    schedule: &'a mut Schedule,
    registry: &'a mut StateRegistry,
    role: &'a Role,
    facts: &'a MatchFacts<'a>,
}

impl<'a> Installing<'a> {
    pub(super) const fn new(
        world: &'a mut World,
        schedule: &'a mut Schedule,
        registry: &'a mut StateRegistry,
        role: &'a Role,
        facts: &'a MatchFacts<'a>,
    ) -> Installing<'a> {
        Installing {
            world,
            schedule,
            registry,
            role,
            facts,
        }
    }

    /// Installs `M`: its declarations, then its systems.
    pub(super) fn install<M: Module>(&mut self) {
        let set = self.facts.set;
        let mut installer =
            Installer::new(self.world, self.schedule, self.registry, set, self.role);
        Declaring::new(set, &mut installer).declare::<M>();
        M::install(&mut InstallCx::new(self.schedule, self.facts));
    }
}

impl fmt::Debug for Installing<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installing")
            .field("role", &self.role)
            .field("facts", &self.facts)
            .finish_non_exhaustive()
    }
}

impl ModuleVisitor for Installing<'_> {
    fn module<M: Module>(&mut self) {
        if self.facts.set.runs(M::CAPABILITY) {
            self.install::<M>();
        }
    }
}
