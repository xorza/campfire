use campfire_core::{CapabilitySet, Declarations, Module, ModuleVisitor};

/// The walk that visits, with `D`, the declarations of each module a match of its set runs.
#[derive(Debug)]
pub(super) struct Declaring<'a, D> {
    set: CapabilitySet,
    d: &'a mut D,
}

impl<'a, D: Declarations> Declaring<'a, D> {
    pub(super) const fn new(set: CapabilitySet, d: &'a mut D) -> Declaring<'a, D> {
        Declaring { set, d }
    }

    /// Visits the declarations of `M`, after the mark of its start.
    pub(super) fn declare<M: Module>(&mut self) {
        self.d.module(M::CAPABILITY);
        M::declare(self.d);
    }
}

impl<D: Declarations> ModuleVisitor for Declaring<'_, D> {
    fn module<M: Module>(&mut self) {
        if self.set.runs(M::CAPABILITY) {
            self.declare::<M>();
        }
    }
}
