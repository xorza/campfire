//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use campfire_declarations::{Book, BuiltBooks, CapabilitySet};
use campfire_mode::{MarkerSpec, ModeInputs};

pub(crate) mod book_cells;
pub(crate) mod book_input;
pub(crate) mod load_names;
pub(crate) mod package_content;
pub(crate) mod units_seed;

/// The books of a match, built once from its packages at its tick rate, each until the build
/// puts it where its owner declares it, before its owner installs; what the mode's load takes;
/// and the capabilities whose modules built them, which the match runs.
#[derive(Debug)]
pub struct Books {
    pub(crate) built: BuiltBooks,
    pub(crate) inputs: ModeInputs,
    pub(crate) set: CapabilitySet,
}

impl Books {
    /// The capabilities whose modules built the books, which a match of them runs.
    pub const fn set(&self) -> CapabilitySet {
        self.set
    }

    /// The book of type `B`, when a module of the match declares it.
    pub fn get<B: Book>(&self) -> Option<&B> {
        self.built.get::<B>()
    }

    /// The markers of the match's map, in the map's order.
    pub fn markers(&self) -> &[MarkerSpec] {
        &self.inputs.load.map.markers
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::any::type_name;

    use campfire_declarations::Book;
    use campfire_mode::ModeUnits;

    use crate::books::Books;

    impl Books {
        /// The units the mode spawns, which a test that starts no mode spawns itself.
        pub const fn mode_units(&self) -> &ModeUnits {
            &self.inputs.units
        }

        /// Puts `book` in place of the one built, as a test gives its own before the build.
        pub fn replace<B: Book>(&mut self, book: B) {
            let built = self.built.take::<B>();
            assert!(built.is_some(), "{} is built", type_name::<B>());
            self.built.add(book);
        }
    }
}
