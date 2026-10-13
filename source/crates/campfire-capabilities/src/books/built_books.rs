use std::any::{Any, type_name};
use std::fmt;

use crate::declare::book::Book;

/// The books the builder made, each once, until the install takes each where its owner declares
/// it.
#[derive(Default)]
pub(crate) struct BuiltBooks(Vec<Built>);

/// A book the builder made, with its type's name, which an error of the install names.
#[derive(Debug)]
struct Built {
    name: &'static str,
    book: Box<dyn Any + Send + Sync>,
}

impl BuiltBooks {
    /// Adds `book`, the first of its type.
    pub(crate) fn add<B: Book>(&mut self, book: B) {
        assert!(
            self.get::<B>().is_none(),
            "{} is built once",
            type_name::<B>()
        );
        self.0.push(Built {
            name: type_name::<B>(),
            book: Box::new(book),
        });
    }

    /// The book of type `B`, if it was built.
    pub(crate) fn get<B: Book>(&self) -> Option<&B> {
        self.0.iter().find_map(|built| built.book.downcast_ref())
    }

    /// Takes the book of type `B` out, if it was built.
    pub(crate) fn take<B: Book>(&mut self) -> Option<B> {
        let at = self.0.iter().position(|built| built.book.is::<B>())?;
        let built = self.0.swap_remove(at).book;
        Some(*built.downcast().expect("the book is of its type"))
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The names of the books it holds.
impl fmt::Debug for BuiltBooks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.0.iter().map(|built| built.name))
            .finish()
    }
}
