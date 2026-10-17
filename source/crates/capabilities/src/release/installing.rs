use std::fmt;

use campfire_declare::{InstallCx, InstallFacts, Module, ModuleVisitor};
use campfire_sim::SimParts;

use crate::books::built_books::BuiltBooks;
use crate::release::declaring::{Declaring, Scripts};
use crate::release::installer::{InstallParts, Installer};

/// The walk that installs each module a match runs, as `facts` says: its declarations, then its
/// systems; at the match's build, every module but the mode's scripted part, which a second walk
/// installs alone once the scripts compile.
pub(super) struct Installing<'a> {
    parts: SimParts<'a>,
    books: &'a mut BuiltBooks,
    facts: &'a InstallFacts<'a>,
    walk: Walk,
}

/// Which modules of the list a walk installs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Walk {
    /// Every module but the mode's scripted part, as the match is built.
    Build,
    /// The mode's scripted part alone, once the scripts compile.
    Scripted,
}

impl<'a> Installing<'a> {
    /// The walk of `walk` into the match of `parts`, its books from `books`.
    pub(super) const fn new(
        parts: SimParts<'a>,
        books: &'a mut BuiltBooks,
        facts: &'a InstallFacts<'a>,
        walk: Walk,
    ) -> Self {
        Installing {
            parts,
            books,
            facts,
            walk,
        }
    }

    /// Installs `M`: its declarations, then its systems.
    fn install<M: Module>(&mut self) {
        let SimParts {
            world,
            schedule,
            registry,
        } = &mut self.parts;
        let parts = InstallParts {
            world,
            schedule,
            registry,
            books: self.books,
        };
        let mut installer = Installer::new(parts, self.facts);
        Declaring::new(self.facts.facts.set, Scripts::Visited, &mut installer).declare::<M>();
        M::install(&mut InstallCx::<M::Set>::new(schedule, self.facts, world));
    }
}

impl fmt::Debug for Installing<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installing")
            .field("facts", &self.facts)
            .field("walk", &self.walk)
            .finish_non_exhaustive()
    }
}

impl ModuleVisitor for Installing<'_> {
    fn module<M: Module>(&mut self) {
        if self.walk == Walk::Build && self.facts.facts.set.runs(M::CAPABILITY) {
            self.install::<M>();
        }
    }

    fn scripted<M: Module>(&mut self) {
        if self.walk == Walk::Scripted {
            self.install::<M>();
        }
    }
}
