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
            Self::OnResolve => 0,
            Self::OnHit => 1,
            Self::OnEnd => 2,
        }
    }

    /// The scripts' hook it runs before.
    pub const fn hook(self) -> Hook {
        match self {
            Self::OnResolve => Hook::OnResolve,
            Self::OnHit => Hook::OnHit,
            Self::OnEnd => Hook::OnEnd,
        }
    }

    /// Its list's key in data, its hook's name.
    pub const fn name(self) -> &'static str {
        self.hook().name()
    }
}
