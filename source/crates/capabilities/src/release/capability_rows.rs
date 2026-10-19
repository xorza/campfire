use campfire_declare::{Module, ModuleVisitor};
use campfire_sim::Capability;

/// Each capability of the release's list, in the order they install, with what it builds on.
#[derive(Debug, Default)]
pub(super) struct CapabilityRows(pub(super) Vec<CapabilityRow>);

/// A capability of the list, and the capabilities it builds on.
#[derive(Debug, Clone, Copy)]
pub(super) struct CapabilityRow {
    pub(super) capability: Capability,
    pub(super) needs: &'static [Capability],
}

impl CapabilityRows {
    /// What `capability` builds on; none for the mode, which is no row.
    pub(super) fn needs(&self, capability: Capability) -> Option<&'static [Capability]> {
        self.0
            .iter()
            .find(|row| row.capability == capability)
            .map(|row| row.needs)
    }
}

/// The mode is no row: every match has it, and a mode declares only the others.
impl ModuleVisitor for CapabilityRows {
    fn module<M: Module>(&mut self) {
        if let Some(capability) = M::CAPABILITY.filter(|&capability| capability != Capability::Mode)
        {
            self.0.push(CapabilityRow {
                capability,
                needs: M::NEEDS,
            });
        }
    }

    fn scripted<M: Module>(&mut self) {}
}
