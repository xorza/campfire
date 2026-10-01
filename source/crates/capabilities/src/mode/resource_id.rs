use crate::values::declared_name::DeclaredName;

/// A player resource, by its place in the mode's `resources`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourceId(u8);

/// An amount of one player resource, as an action's cost names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceAmount {
    pub resource: ResourceId,
    pub amount: i64,
}

impl ResourceId {
    /// The most player resources a mode declares: every index a `u8` holds.
    pub const LIMIT: usize = 256;

    /// The resource `name` among the mode's `resources`; `None` when the mode does not declare
    /// it.
    pub fn of(resources: &[DeclaredName], name: &str) -> Option<ResourceId> {
        let at = resources.iter().position(|held| held.as_str() == name)?;
        Some(ResourceId(u8::try_from(at).ok()?))
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}
