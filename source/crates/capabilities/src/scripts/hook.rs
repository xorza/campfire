use campfire_sim::Capability;

/// A hook the engine calls in a script, by name: every hook of the script API, whether the
/// release calls it yet or not, so the package load checks know them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    OnResolve,
    OnHit,
    OnEnd,
    OnChannelTick,
    OnInterrupt,
    OnInterval,
    OnAttack,
    OnAttackHit,
    OnDamageTaken,
    OnKill,
    OnTakedown,
    OnMatchStart,
    OnModeInput,
    OnTimer,
    OnPlayerJoin,
    OnPlayerLeave,
    OnUnitDied,
    CalcDamage,
    CalcHeal,
    OnThink,
    OnLevelUp,
}

/// What a script serves, as the data that names it says: each role has its own hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScriptRole {
    Action,
    Modifier,
    Mode,
    Ai,
}

impl ScriptRole {
    pub const ALL: [ScriptRole; 4] = [
        ScriptRole::Action,
        ScriptRole::Modifier,
        ScriptRole::Mode,
        ScriptRole::Ai,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ScriptRole::Action => "action",
            ScriptRole::Modifier => "modifier",
            ScriptRole::Mode => "mode",
            ScriptRole::Ai => "AI",
        }
    }
}

impl Hook {
    pub const ALL: [Hook; 21] = [
        Hook::OnResolve,
        Hook::OnHit,
        Hook::OnEnd,
        Hook::OnChannelTick,
        Hook::OnInterrupt,
        Hook::OnInterval,
        Hook::OnAttack,
        Hook::OnAttackHit,
        Hook::OnDamageTaken,
        Hook::OnKill,
        Hook::OnTakedown,
        Hook::OnMatchStart,
        Hook::OnModeInput,
        Hook::OnTimer,
        Hook::OnPlayerJoin,
        Hook::OnPlayerLeave,
        Hook::OnUnitDied,
        Hook::CalcDamage,
        Hook::CalcHeal,
        Hook::OnThink,
        Hook::OnLevelUp,
    ];

    /// The prefixes that mark a script function as a hook: one with either that is no hook of
    /// the API is a misspelling, not a helper.
    pub const PREFIXES: [&'static str; 2] = ["on_", "calc_"];

    /// The hook named `name`, of any role.
    pub fn named(name: &str) -> Option<Hook> {
        Hook::ALL.into_iter().find(|hook| hook.name() == name)
    }

    /// The script function the engine calls.
    pub const fn name(self) -> &'static str {
        match self {
            Hook::OnResolve => "on_resolve",
            Hook::OnHit => "on_hit",
            Hook::OnEnd => "on_end",
            Hook::OnChannelTick => "on_channel_tick",
            Hook::OnInterrupt => "on_interrupt",
            Hook::OnInterval => "on_interval",
            Hook::OnAttack => "on_attack",
            Hook::OnAttackHit => "on_attack_hit",
            Hook::OnDamageTaken => "on_damage_taken",
            Hook::OnKill => "on_kill",
            Hook::OnTakedown => "on_takedown",
            Hook::OnMatchStart => "on_match_start",
            Hook::OnModeInput => "on_mode_input",
            Hook::OnTimer => "on_timer",
            Hook::OnPlayerJoin => "on_player_join",
            Hook::OnPlayerLeave => "on_player_leave",
            Hook::OnUnitDied => "on_unit_died",
            Hook::CalcDamage => "calc_damage",
            Hook::CalcHeal => "calc_heal",
            Hook::OnThink => "on_think",
            Hook::OnLevelUp => "on_level_up",
        }
    }

    /// The names of the function's parameters, `ctx` first, as the reference shows them.
    pub(crate) const fn param_names(self) -> &'static [&'static str] {
        match self {
            Hook::OnMatchStart => &["ctx"],
            Hook::OnChannelTick | Hook::OnThink => &["ctx", "unit"],
            Hook::OnInterval => &["ctx", "m"],
            Hook::OnPlayerJoin | Hook::OnPlayerLeave => &["ctx", "player"],
            Hook::CalcDamage => &["ctx", "d"],
            Hook::CalcHeal => &["ctx", "h"],
            Hook::OnResolve | Hook::OnInterrupt => &["ctx", "unit", "target"],
            Hook::OnEnd => &["ctx", "unit", "hit"],
            Hook::OnAttack => &["ctx", "m", "target"],
            Hook::OnAttackHit | Hook::OnDamageTaken => &["ctx", "m", "d"],
            Hook::OnKill | Hook::OnTakedown => &["ctx", "m", "victim"],
            Hook::OnTimer => &["ctx", "name", "data"],
            Hook::OnHit => &["ctx", "unit", "target", "hit"],
            Hook::OnModeInput => &["ctx", "player", "name", "value"],
            Hook::OnUnitDied => &["ctx", "unit", "killer", "assisters"],
            Hook::OnLevelUp => &["ctx", "unit", "track", "level"],
        }
    }

    /// How many parameters the function takes, `ctx` first.
    pub const fn params(self) -> usize {
        self.param_names().len()
    }

    pub const fn role(self) -> ScriptRole {
        match self {
            Hook::OnResolve
            | Hook::OnHit
            | Hook::OnEnd
            | Hook::OnChannelTick
            | Hook::OnInterrupt => ScriptRole::Action,
            Hook::OnInterval
            | Hook::OnAttack
            | Hook::OnAttackHit
            | Hook::OnDamageTaken
            | Hook::OnKill
            | Hook::OnTakedown => ScriptRole::Modifier,
            Hook::OnMatchStart
            | Hook::OnModeInput
            | Hook::OnTimer
            | Hook::OnPlayerJoin
            | Hook::OnPlayerLeave
            | Hook::OnUnitDied
            | Hook::CalcDamage
            | Hook::CalcHeal
            | Hook::OnLevelUp => ScriptRole::Mode,
            Hook::OnThink => ScriptRole::Ai,
        }
    }

    /// The capability that calls it; `None` for the core's.
    pub const fn capability(self) -> Option<Capability> {
        match self {
            Hook::OnResolve
            | Hook::OnHit
            | Hook::OnEnd
            | Hook::OnChannelTick
            | Hook::OnInterrupt => Some(Capability::Abilities),
            Hook::OnInterval => Some(Capability::Stats),
            Hook::OnAttack
            | Hook::OnAttackHit
            | Hook::OnDamageTaken
            | Hook::OnKill
            | Hook::OnTakedown
            | Hook::OnUnitDied
            | Hook::CalcDamage
            | Hook::CalcHeal => Some(Capability::Combat),
            Hook::OnMatchStart
            | Hook::OnModeInput
            | Hook::OnTimer
            | Hook::OnPlayerJoin
            | Hook::OnPlayerLeave => None,
            Hook::OnThink => Some(Capability::Orders),
            Hook::OnLevelUp => Some(Capability::Progression),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::relation::Relation;

    #[test]
    fn every_hook_is_found_by_its_name_alone() {
        for hook in Hook::ALL {
            assert_eq!(Hook::named(hook.name()), Some(hook));
        }
        assert_eq!(Hook::named("on_cats"), None);
        assert_eq!(Hook::named("on_cast"), None);
        assert_eq!(Hook::named("think"), None);
        let named_like_hooks = Hook::ALL.iter().all(|hook| {
            Hook::PREFIXES
                .iter()
                .any(|prefix| hook.name().starts_with(prefix))
        });
        assert!(named_like_hooks);
        for relation in [Relation::Enemies, Relation::Allies, Relation::All] {
            assert_eq!(Relation::named(relation.name()), Some(relation));
        }
        // A few as design 08 lists them.
        let think = Hook::OnThink;
        assert_eq!((think.params(), think.role()), (2, ScriptRole::Ai));
        assert_eq!(think.capability(), Some(Capability::Orders));
        let hit = Hook::OnHit;
        assert_eq!((hit.params(), hit.role()), (4, ScriptRole::Action));
        assert_eq!(Hook::CalcHeal.capability(), Some(Capability::Combat));
        assert_eq!(Hook::OnModeInput.params(), 4);
        assert_eq!(Hook::OnTimer.capability(), None);
    }
}
