use campfire_sim::Capability;

/// Data a by-type book holds for each unit type that has it: whose book it is, as `Book` says of
/// a book.
pub(crate) trait PerType: Send + Sync + 'static {
    /// The capability that declares the book; none for the core.
    const CAPABILITY: Option<Capability>;
    /// Whether only a match that runs scripts reads the book.
    const SCRIPTS_ONLY: bool;
}
