use std::iter::Copied;
use std::slice;

use crate::scripts::hook::Hook;
use crate::scripts::script_role::ScriptRole;

/// A set of hooks, as the hooks a script defines, each once, by its name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookSet(Box<[Hook]>);

impl FromIterator<Hook> for HookSet {
    fn from_iter<I: IntoIterator<Item = Hook>>(hooks: I) -> Self {
        let mut hooks: Vec<_> = hooks.into_iter().collect();
        hooks.sort_unstable_by_key(|hook| hook.name);
        hooks.dedup_by_key(|hook| hook.name);
        Self(hooks.into())
    }
}

impl<const N: usize> From<[Hook; N]> for HookSet {
    fn from(hooks: [Hook; N]) -> Self {
        hooks.into_iter().collect()
    }
}

impl HookSet {
    /// The hooks, by name.
    pub fn iter(&self) -> Copied<slice::Iter<'_, Hook>> {
        self.0.iter().copied()
    }

    pub fn contains(&self, hook: Hook) -> bool {
        self.0.iter().any(|held| held.name == hook.name)
    }

    /// The hooks of `role` it holds.
    #[must_use]
    pub fn of_role(&self, role: ScriptRole) -> Self {
        self.0
            .iter()
            .copied()
            .filter(|hook| hook.role == role)
            .collect()
    }
}

impl<'a> IntoIterator for &'a HookSet {
    type Item = Hook;
    type IntoIter = Copied<slice::Iter<'a, Hook>>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests;
