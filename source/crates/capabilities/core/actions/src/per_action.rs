/// A value a book of actions holds, `ByAction`.
pub trait PerAction: Send + Sync + 'static {
    /// Whether only a match that runs scripts reads the book.
    const SCRIPTS_ONLY: bool;
}
