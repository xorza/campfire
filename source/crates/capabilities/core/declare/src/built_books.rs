use std::any::{Any, type_name};
use std::fmt;

use crate::book::Book;

/// The books the build made, each once, until the install takes each where its owner declares
/// it.
#[derive(Default)]
pub struct BuiltBooks(Vec<Built>);

/// A book the builder made, with its type's name, which an error of the install names.
#[derive(Debug)]
struct Built {
    name: &'static str,
    book: Box<dyn Any + Send + Sync>,
}

impl BuiltBooks {
    /// Adds `book`, the first of its type.
    pub fn add<B: Book>(&mut self, book: B) {
        assert!(
            self.get::<B>().is_none(),
            "{} is built once",
            type_name::<B>()
        );
        self.add_boxed(type_name::<B>(), Box::new(book));
    }

    /// Adds `book`, of the type `name` names, as a build holds it.
    pub(crate) fn add_boxed(&mut self, name: &'static str, book: Box<dyn Any + Send + Sync>) {
        self.0.push(Built { name, book });
    }

    /// The book of type `B`, if it was built.
    pub fn get<B: Book>(&self) -> Option<&B> {
        self.0.iter().find_map(|built| built.book.downcast_ref())
    }

    /// Takes the book of type `B` out, if it was built.
    pub fn take<B: Book>(&mut self) -> Option<B> {
        let at = self.0.iter().position(|built| built.book.is::<B>())?;
        let built = self.0.swap_remove(at).book;
        Some(*built.downcast().expect("the book is of its type"))
    }

    pub const fn is_empty(&self) -> bool {
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
