use bevy_ecs::resource::Resource;
use campfire_sim::Capability;

use crate::declare::capability_set::CapabilitySet;
use crate::scripts::frame::Frame;
use crate::units::view::View;

/// Package data a module reads, which its declaration puts in a match empty, and the install
/// fills from the packages, each book through `Books::load`. The module that declares it is its
/// owner: the declaration audit holds the two to agree.
pub(crate) trait Book: Resource + Default {
    /// The capability that declares it; none for the core.
    const OWNER: Option<Capability>;
    /// Whether only a match that runs scripts reads it, so a client holds none.
    const SCRIPTS_ONLY: bool;

    /// Gives the script view and, in a match that runs scripts, the call frame what they copy
    /// of it.
    fn share(&self, view: &View, frame: Option<&mut Frame>);

    /// Whether a match of the capabilities `set`, which runs scripts when `scripts` says, holds
    /// it: one of its owner's, as it reads it.
    fn held(set: CapabilitySet, scripts: bool) -> bool {
        set.runs(Self::OWNER) && (scripts || !Self::SCRIPTS_ONLY)
    }
}
