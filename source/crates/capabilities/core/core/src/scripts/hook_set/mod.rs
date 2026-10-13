use crate::scripts::hook::Hook;

/// A set of hooks, one bit each, as the hooks a script defines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HookSet(u32);

impl FromIterator<Hook> for HookSet {
    fn from_iter<I: IntoIterator<Item = Hook>>(hooks: I) -> HookSet {
        hooks
            .into_iter()
            .fold(HookSet(0), |set, hook| HookSet(set.0 | HookSet::bit(hook)))
    }
}

impl<const N: usize> From<[Hook; N]> for HookSet {
    fn from(hooks: [Hook; N]) -> HookSet {
        hooks.into_iter().collect()
    }
}

impl HookSet {
    pub const fn contains(self, hook: Hook) -> bool {
        self.0 & HookSet::bit(hook) != 0
    }

    const fn bit(hook: Hook) -> u32 {
        1 << hook as u32
    }
}

#[cfg(test)]
mod tests;
