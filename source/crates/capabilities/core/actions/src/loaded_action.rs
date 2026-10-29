use std::error::Error;

use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, BookPlace, Section};
use campfire_names::UnknownName;
use campfire_values::{ActionId, PackageIndex};

use crate::action_book::ActionBook;
use crate::action_data::ActionData;
use crate::actions_data::ActionsData;
use crate::kind_section::KindSection;

/// An action of a package that the match loaded: its package's place, its id there, its id in
/// the match, and its data. What a kind's capability builds its book from, its section's values
/// resolved against the match (04-capabilities/actions.md, Each kind owns its section).
#[derive(Debug, Clone, Copy)]
pub struct LoadedAction<'c> {
    pub package: PackageIndex,
    pub name: &'c DeclaredName,
    pub action: ActionId,
    pub data: &'c ActionData,
}

impl<'c> LoadedAction<'c> {
    /// Every package's actions the match loaded, package by package, each in the order of its
    /// ids; a rules package's, which no match loads, are none of them.
    pub fn all(cx: &'c BookCx<'_>) -> Result<Vec<Self>, BookError> {
        let book = cx.book::<ActionBook>()?;
        let mut actions = Vec::new();
        for (package, source) in cx.packages() {
            let data = source.content.get::<ActionsData>().into_iter();
            for (name, data) in data.flat_map(|data| &data.0) {
                let Some(action) = book.names().named((package, name.as_str())) else {
                    continue;
                };
                actions.push(Self {
                    package,
                    name,
                    action,
                    data,
                });
            }
        }
        Ok(actions)
    }

    /// Its kind's section, when its kind's is an `S`.
    pub fn section<S: KindSection>(self) -> Option<&'c S> {
        self.data.section::<S>()
    }

    /// Its place in its package.
    pub fn place(self) -> BookPlace {
        BookPlace::Entry {
            table: ActionsData::KEY,
            name: self.name.clone(),
        }
    }

    /// The error of `name`, which it names and the match does not have.
    pub fn unknown(self, name: UnknownName) -> BookError {
        BookError::unknown(self.package, self.place(), name)
    }

    /// The error of `problem`, a rule of its kind's capability that it breaks.
    pub fn broken<E: Error + Clone + PartialEq + Send + Sync + 'static>(
        self,
        problem: E,
    ) -> BookError {
        BookError::rule(self.package, self.place(), problem)
    }
}
