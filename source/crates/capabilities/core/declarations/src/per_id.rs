/// Data a by-id book of ids of `K` holds for each id that has it: who reads the book, as `Book`
/// says of a book.
pub trait PerId<K>: Send + Sync + 'static {
    /// Whether only a match that runs scripts reads the book.
    const SCRIPTS_ONLY: bool;
}
