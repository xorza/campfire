use campfire_script::Hook;

/// The hook an action's effect list runs before: its resolve's, or its delivery's hit or end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListHook {
    OnResolve,
    OnHit,
    OnEnd,
}

impl ListHook {
    /// Its place among an action's lists.
    pub const fn index(self) -> usize {
        match self {
            ListHook::OnResolve => 0,
            ListHook::OnHit => 1,
            ListHook::OnEnd => 2,
        }
    }

    /// The scripts' hook it runs before.
    pub const fn hook(self) -> Hook {
        match self {
            ListHook::OnResolve => Hook::OnResolve,
            ListHook::OnHit => Hook::OnHit,
            ListHook::OnEnd => Hook::OnEnd,
        }
    }

    /// Its list's key in data, its hook's name.
    pub const fn name(self) -> &'static str {
        self.hook().name()
    }
}
