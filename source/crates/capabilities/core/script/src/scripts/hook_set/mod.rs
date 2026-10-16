use crate::scripts::hook::Hook;

/// A set of hooks, one bit each, as the hooks a script defines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HookSet(u32);

impl FromIterator<Hook> for HookSet {
    fn from_iter<I: IntoIterator<Item = Hook>>(hooks: I) -> Self {
        hooks
            .into_iter()
            .fold(Self(0), |set, hook| Self(set.0 | Self::bit(hook)))
    }
}

impl<const N: usize> From<[Hook; N]> for HookSet {
    fn from(hooks: [Hook; N]) -> Self {
        hooks.into_iter().collect()
    }
}

impl HookSet {
    pub const fn contains(self, hook: Hook) -> bool {
        self.0 & Self::bit(hook) != 0
    }

    const fn bit(hook: Hook) -> u32 {
        1 << hook as u32
    }
}

#[cfg(test)]
mod tests;
