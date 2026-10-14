use campfire_declare::{InstallCx, Module, ModuleVisitor};
use campfire_sim::StateRegistry;

use crate::release::declaring::Declaring;
use crate::release::installer::Installer;

/// The walk that installs each module a match runs: its declarations, then its systems.
#[derive(Debug)]
pub(super) struct Installing<'a, 'w> {
    cx: &'a mut InstallCx<'w>,
    registry: &'a mut StateRegistry,
}

impl<'a, 'w> Installing<'a, 'w> {
    pub(super) const fn new(
        cx: &'a mut InstallCx<'w>,
        registry: &'a mut StateRegistry,
    ) -> Installing<'a, 'w> {
        Installing { cx, registry }
    }

    /// Installs `M`: its declarations, then its systems.
    pub(super) fn install<M: Module>(&mut self) {
        let set = self.cx.set();
        let mut installer = Installer::new(self.cx, self.registry);
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
