use campfire_declare::{InstallCx, Module, ModuleVisitor, Role};
use campfire_sim::StateRegistry;

use crate::release::declaring::Declaring;
use crate::release::installer::Installer;

/// The walk that installs each module a match runs, of `role`: its declarations, then its
/// systems.
#[derive(Debug)]
pub(super) struct Installing<'a, 'w> {
    cx: &'a mut InstallCx<'w>,
    registry: &'a mut StateRegistry,
    role: &'a Role,
}

impl<'a, 'w> Installing<'a, 'w> {
    pub(super) const fn new(
        cx: &'a mut InstallCx<'w>,
        registry: &'a mut StateRegistry,
        role: &'a Role,
    ) -> Installing<'a, 'w> {
        Installing { cx, registry, role }
    }

    /// Installs `M`: its declarations, then its systems.
    pub(super) fn install<M: Module>(&mut self) {
        let set = self.cx.set();
        let mut installer = Installer::new(self.cx, self.registry, self.role);
        Declaring::new(set, &mut installer).declare::<M>();
        M::install(self.cx);
    }
}

impl ModuleVisitor for Installing<'_, '_> {
    fn module<M: Module>(&mut self) {
        if self.cx.set().runs(M::CAPABILITY) {
            self.install::<M>();
        }
    }
}
