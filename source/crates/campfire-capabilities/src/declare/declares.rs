use campfire_sim::Capability;

use crate::declare::declarations::Declarations;

/// A module of the engine, the core's or a capability's: what it adds to a match but its
/// systems, declared once, which every consumer of `Declarations` reads.
pub(crate) trait Declares {
    /// The capability it is; none for the core.
    const MODULE: Option<Capability>;

    fn declare<D: Declarations>(d: &mut D);
}
