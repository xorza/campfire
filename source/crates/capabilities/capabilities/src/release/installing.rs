use std::fmt;

use campfire_declarations::{BuiltBooks, InstallCx, InstallFacts, Module, ModuleVisitor};
use campfire_sim::SimFields;

use crate::release::declaring::{Declaring, Scripts};
use crate::release::installer::{InstallFields, Installer};

/// The walk that installs each module a match runs, as `facts` says, in two passes: its
/// declarations, which put its books in, then its systems, which the books' facts decide; the
/// mode's scripted part apart, which a third walk installs whole once the scripts compile.
pub(super) struct Installing<'a> {
    fields: SimFields<'a>,
    books: &'a mut BuiltBooks,
    facts: &'a InstallFacts<'a>,
    walk: Walk,
}

/// Which modules of the list a walk installs, and what of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Walk {
    /// The declarations of every module but the mode's scripted part, as the match is built.
    Declare,
    /// The systems of every module but the mode's scripted part, once every book is in place
    /// (02-engine-core.md, Books before the install).
    Install,
    /// The mode's scripted part alone, its declarations then its systems, once the scripts
    /// compile.
    Scripted,
}

impl<'a> Installing<'a> {
    /// The walk of `walk` into the match of `fields`, its books from `books`.
    pub(super) const fn new(
        fields: SimFields<'a>,
        books: &'a mut BuiltBooks,
        facts: &'a InstallFacts<'a>,
        walk: Walk,
    ) -> Self {
        Installing {
            fields,
            books,
            facts,
            walk,
        }
    }

    /// Puts in the declarations of `M`.
    fn declare<M: Module>(&mut self) {
        let SimFields {
            world,
            schedule,
            registry,
        } = &mut self.fields;
        let fields = InstallFields {
            world,
            schedule,
            registry,
            books: self.books,
        };
        let mut installer = Installer::new(fields, self.facts);
        Declaring::new(self.facts.facts.set, Scripts::Visited, &mut installer).declare::<M>();
    }

    /// Adds the systems of `M`.
    fn install<M: Module>(&mut self) {
        let SimFields {
            world, schedule, ..
        } = &mut self.fields;
        M::install(&mut InstallCx::<M::Systems>::new(
            schedule, self.facts, world,
        ));
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
        if self.facts.facts.set.runs(M::CAPABILITY) {
            match self.walk {
                Walk::Declare => self.declare::<M>(),
                Walk::Install => self.install::<M>(),
                Walk::Scripted => {}
            }
        }
    }

    fn scripted<M: Module>(&mut self) {
        if self.walk == Walk::Scripted {
            self.declare::<M>();
            self.install::<M>();
        }
    }
}
