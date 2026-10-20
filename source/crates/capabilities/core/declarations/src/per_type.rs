/// Data a by-type book holds for each unit type that has it: who reads the book, as `Book` says of
/// a book.
pub trait PerType: Send + Sync + 'static {
    /// Whether only a match that runs scripts reads the book.
    const SCRIPTS_ONLY: bool;
}
