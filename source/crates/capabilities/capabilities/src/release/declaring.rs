use campfire_declarations::{CapabilitySet, Declarations, Module, ModuleVisitor};

/// The walk that visits, with `D`, the declarations of each module a match of its set runs, the
/// mode's scripted part when `scripts` says.
#[derive(Debug)]
pub(super) struct Declaring<'a, D> {
    set: CapabilitySet,
    scripts: Scripts,
    d: &'a mut D,
}

/// Whether a walk visits the mode's scripted part.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Scripts {
    /// Every declaration a match of the set may hold, as the lists of its state types are.
    Visited,
    /// A match that runs no scripts, as one whose callbacks are counted.
    Skipped,
}

impl<'a, D: Declarations> Declaring<'a, D> {
    pub(super) const fn new(set: CapabilitySet, scripts: Scripts, d: &'a mut D) -> Self {
        Declaring { set, scripts, d }
    }

    /// Visits the declarations of `M`, after the mark of its start and its sets.
    pub(super) fn declare<M: Module>(&mut self) {
        self.d.module(M::CAPABILITY);
        self.d.sets::<M::Systems>();
        M::declare(self.d);
    }
}

impl<D: Declarations> ModuleVisitor for Declaring<'_, D> {
    fn module<M: Module>(&mut self) {
        if self.set.runs(M::CAPABILITY) {
            self.declare::<M>();
        }
    }

    fn scripted<M: Module>(&mut self) {
        if self.scripts == Scripts::Visited {
            self.declare::<M>();
        }
    }
}
