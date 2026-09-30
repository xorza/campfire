use crate::scripts::hook::Hook;

/// A set of hooks, one bit each, as the hooks a script defines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HookSet(u32);

impl HookSet {
    pub(crate) fn of(hooks: impl IntoIterator<Item = Hook>) -> HookSet {
        hooks
            .into_iter()
            .fold(HookSet(0), |set, hook| HookSet(set.0 | HookSet::bit(hook)))
    }

    pub(crate) const fn contains(self, hook: Hook) -> bool {
        self.0 & HookSet::bit(hook) != 0
    }

    const fn bit(hook: Hook) -> u32 {
        1 << hook as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_holds_exactly_its_hooks() {
        let set = HookSet::of([Hook::OnTimer, Hook::OnUnitDied]);
        let held: Vec<_> = Hook::ALL
            .into_iter()
            .filter(|&hook| set.contains(hook))
            .collect();
        assert_eq!(held, [Hook::OnTimer, Hook::OnUnitDied]);
        assert!(
            Hook::ALL
                .into_iter()
                .all(|hook| !HookSet::default().contains(hook))
        );
        assert!(Hook::ALL.len() <= 32, "a hook's bit fits a u32");
    }
}
