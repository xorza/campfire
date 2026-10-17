//! What a match is built from: each package's content, as the load reads it, and the books a
//! match reads, built from it with no world.

use std::num::NonZeroU32;

use campfire_common::DeclaredName;
use campfire_declare::{
    BookCx, BookError, BookSource, BuiltBooks, CapabilitySet, Given, Sections, SourcePackage,
};
use campfire_mode::ModeInputs;
use campfire_sim::TickRate;
use campfire_units::UnitTypes;
use campfire_values::EngineTag;

pub(crate) mod book_cells;
pub(crate) mod book_input;
pub(crate) mod leftover_books;
pub(crate) mod package_content;
#[cfg(test)]
pub(crate) mod test_package;
pub(crate) mod units_seed;

/// The books of a match, built once from its packages at its tick rate, each until the build
/// puts it where its owner declares it, before its owner installs; and what the mode's load
/// takes.
#[derive(Debug)]
pub struct Books {
    pub(crate) built: BuiltBooks,
    pub(crate) inputs: ModeInputs,
}

impl Books {
    /// The unit types of `packages`, the mode's then each dependency's, and the tags of the
    /// match: `engine_tags`, the release's, then `tag_names`, then those the types name, in the
    /// order their ids follow; an error for a type its scope names already, or past the most
    /// types or tags a match holds. The tables a check of the packages resolves their names by.
    pub fn unit_types(
        engine_tags: &[EngineTag],
        tag_names: &[DeclaredName],
        packages: &[SourcePackage<'_>],
    ) -> Result<UnitTypes, BookError> {
        let none = Sections::default();
        let source = BookSource {
            rate: TickRate::new(NonZeroU32::MIN),
            set: CapabilitySet::CORE,
            engine_tags,
            tag_names,
            mode: &none,
            map: &none,
            packages: packages.to_vec(),
            given: Given::default(),
        };
        UnitTypes::build(&BookCx::new(source))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::any::type_name;

    use campfire_declare::Book;
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
