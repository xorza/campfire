use std::fmt::Debug;

/// What a module's books build on that another module makes from its own data: built when a
/// builder asks for it, as a book is, and never installed, as the match reads none of it.
pub trait Fact: Debug + Send + Sync + 'static {}
