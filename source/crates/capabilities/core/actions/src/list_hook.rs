use campfire_script::{Ctx, Hook, HookDecl, ScriptRole, ScriptValue, Unit};

/// The hook an action's effect list runs before: its resolve's, or its delivery's hit or end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListHook {
    OnResolve,
    OnHit,
    OnEnd,
}

/// `on_resolve(ctx, caster, target)`: an action resolves, from its caster at the unit, the point or
/// nothing it aims at.
#[derive(Debug)]
pub struct OnResolve;

/// `on_hit(ctx, caster, target, hit)`: an action's delivery reaches a unit, from its caster, who
/// may be gone.
#[derive(Debug)]
pub struct OnHit;

/// `on_end(ctx, caster, hit)`: an action's delivery ends, from its caster, who may be gone.
#[derive(Debug)]
pub struct OnEnd;

impl HookDecl for OnResolve {
    const NAME: &'static str = "on_resolve";
    const ROLE: ScriptRole = ScriptRole::Action;
    const PARAMS: &'static [&'static str] = &["ctx", "caster", "target"];
    type Args = (Ctx, Unit, ScriptValue);
}

impl HookDecl for OnHit {
    const NAME: &'static str = "on_hit";
    const ROLE: ScriptRole = ScriptRole::Action;
    const PARAMS: &'static [&'static str] = &["ctx", "caster", "target", "hit"];
    type Args = (Ctx, ScriptValue, ScriptValue, ScriptValue);
}

impl HookDecl for OnEnd {
    const NAME: &'static str = "on_end";
    const ROLE: ScriptRole = ScriptRole::Action;
    const PARAMS: &'static [&'static str] = &["ctx", "caster", "hit"];
    type Args = (Ctx, ScriptValue, ScriptValue);
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
            Self::OnResolve => Hook::of::<OnResolve>(),
            Self::OnHit => Hook::of::<OnHit>(),
            Self::OnEnd => Hook::of::<OnEnd>(),
        }
    }

    /// Its list's key in data, its hook's name.
    pub const fn name(self) -> &'static str {
        self.hook().name
    }
}
