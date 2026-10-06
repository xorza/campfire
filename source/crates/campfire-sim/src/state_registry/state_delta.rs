use crate::stable_id::StableId;

/// The state a world changed between two copies, which `StateRegistry::apply` makes another
/// world follow: the stable ids it lost and gained, then a section for each registered type, in
/// the registry's order: for a component, each changed value as `(id, Some(value))` and each
/// removal from a live entity as `(id, None)`, in postcard; for a resource, nothing when
/// unchanged, else the value as an `Option`, none when it is gone.
#[derive(Debug, Default)]
pub struct StateDelta {
    pub(crate) lost: Vec<StableId>,
    pub(crate) gained: Vec<StableId>,
    pub(crate) bytes: Vec<u8>,
    /// Where each type's section starts in `bytes`, and where the last ends.
    pub(crate) starts: Vec<usize>,
}

impl StateDelta {
    pub(crate) fn clear(&mut self) {
        self.lost.clear();
        self.gained.clear();
        self.bytes.clear();
        self.starts.clear();
        self.starts.push(0);
    }

    /// Ends the section of the next type at the bytes written so far.
    pub(crate) fn end_section(&mut self) {
        self.starts.push(self.bytes.len());
    }

    /// The section of the type at `entry`.
    pub(crate) fn section(&self, entry: usize) -> &[u8] {
        &self.bytes[self.starts[entry]..self.starts[entry + 1]]
    }

    /// How many types it holds a section of.
    pub(crate) const fn sections(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }
}
