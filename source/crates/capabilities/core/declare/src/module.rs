use campfire_sim::Capability;

use crate::declarations::Declarations;
use crate::install_cx::InstallCx;

/// A module of the engine, the core's or a capability's: what it adds to a match but its
/// systems, declared once, which every consumer of `Declarations` reads, and the install of its
/// systems. The release's one list visits the modules in the order they install.
pub trait Module {
    /// The capability it is; none for the core's.
    const CAPABILITY: Option<Capability>;
    /// The capabilities it builds on, which install before it and which a match that declares it
    /// declares: without one, it does not run.
    const NEEDS: &'static [Capability];
    /// The capabilities it reads when the match has them, and runs without. With `NEEDS`, its
    /// crate's dependencies on other capabilities, which a release test holds equal.
    const USES: &'static [Capability];

    fn declare<D: Declarations>(d: &mut D);

    /// Adds its systems to a match, after its declarations.
    fn install(cx: &mut InstallCx<'_>);
}

/// What visits the release's list of modules, each by its type.
pub trait ModuleVisitor {
    fn module<M: Module>(&mut self);
}
