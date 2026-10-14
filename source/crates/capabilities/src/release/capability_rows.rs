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
    /// What `capability` builds on; nothing for one the release does not run yet.
    pub(super) fn needs(&self, capability: Capability) -> &'static [Capability] {
        self.0
            .iter()
            .find(|row| row.capability == capability)
            .map_or(&[], |row| row.needs)
    }
}

impl ModuleVisitor for CapabilityRows {
    fn module<M: Module>(&mut self) {
        if let Some(capability) = M::CAPABILITY {
            self.0.push(CapabilityRow {
                capability,
                needs: M::NEEDS,
            });
        }
    }
}
