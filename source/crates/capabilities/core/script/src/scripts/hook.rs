use serde::Deserialize;

use crate::scripts::script_role::ScriptRole;

campfire_common::enum_all! {
    /// A hook the engine calls in a script, by name: every hook of the script API, whether the
    /// release calls it yet or not, so the package load checks know them all. It reads back from
    /// its `Debug` name, as a log writes it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
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
        OnGenerate,
        OnSystem,
        OnStageDone,
    }
    pub const ALL;
}

impl Hook {
    /// The prefixes that mark a script function as a hook: one with either that is no hook of
    /// the API is a misspelling, not a helper.
    pub const PREFIXES: [&'static str; 2] = ["on_", "calc_"];

    /// The hook named `name`, of any role.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|hook| hook.name() == name)
    }

    /// The script function the engine calls.
    pub const fn name(self) -> &'static str {
        match self {
            Self::OnResolve => "on_resolve",
            Self::OnHit => "on_hit",
            Self::OnEnd => "on_end",
            Self::OnChannelTick => "on_channel_tick",
            Self::OnInterrupt => "on_interrupt",
            Self::OnInterval => "on_interval",
            Self::OnAttack => "on_attack",
            Self::OnAttackHit => "on_attack_hit",
            Self::OnDamageTaken => "on_damage_taken",
            Self::OnKill => "on_kill",
            Self::OnTakedown => "on_takedown",
            Self::OnMatchStart => "on_match_start",
            Self::OnModeInput => "on_mode_input",
            Self::OnTimer => "on_timer",
            Self::OnPlayerJoin => "on_player_join",
            Self::OnPlayerLeave => "on_player_leave",
            Self::OnUnitDied => "on_unit_died",
            Self::CalcDamage => "calc_damage",
            Self::CalcHeal => "calc_heal",
            Self::OnThink => "on_think",
            Self::OnLevelUp => "on_level_up",
            Self::OnGenerate => "on_generate",
            Self::OnSystem => "on_system",
            Self::OnStageDone => "on_stage_done",
        }
    }

    /// The names of the function's parameters, `ctx` first, as the reference shows them.
    pub(crate) const fn param_names(self) -> &'static [&'static str] {
        match self {
            Self::OnMatchStart => &["ctx"],
            Self::OnThink => &["ctx", "unit"],
            Self::OnInterval => &["ctx", "m"],
            Self::OnPlayerJoin | Self::OnPlayerLeave => &["ctx", "player"],
            Self::OnGenerate => &["ctx", "region"],
            Self::OnSystem => &["ctx", "units"],
            Self::CalcDamage => &["ctx", "d"],
            Self::CalcHeal => &["ctx", "h"],
            Self::OnResolve | Self::OnChannelTick | Self::OnInterrupt => &["ctx", "unit", "target"],
            Self::OnEnd => &["ctx", "unit", "hit"],
            Self::OnAttack => &["ctx", "m", "target"],
            Self::OnAttackHit | Self::OnDamageTaken => &["ctx", "m", "d"],
            Self::OnKill | Self::OnTakedown => &["ctx", "m", "victim"],
            Self::OnTimer => &["ctx", "name", "data"],
            Self::OnHit => &["ctx", "unit", "target", "hit"],
            Self::OnModeInput => &["ctx", "player", "name", "value"],
            Self::OnUnitDied => &["ctx", "unit", "killer", "assisters"],
            Self::OnLevelUp => &["ctx", "unit", "track", "level"],
            Self::OnStageDone => &["ctx", "player", "quest", "stage"],
        }
    }

    /// How many parameters the function takes, `ctx` first.
    pub const fn params(self) -> usize {
        self.param_names().len()
    }

    pub const fn role(self) -> ScriptRole {
        match self {
            Self::OnResolve
            | Self::OnHit
            | Self::OnEnd
            | Self::OnChannelTick
            | Self::OnInterrupt => ScriptRole::Action,
            Self::OnInterval
            | Self::OnAttack
            | Self::OnAttackHit
            | Self::OnDamageTaken
            | Self::OnKill
            | Self::OnTakedown => ScriptRole::Modifier,
            Self::OnMatchStart
            | Self::OnModeInput
            | Self::OnTimer
            | Self::OnPlayerJoin
            | Self::OnPlayerLeave
            | Self::OnUnitDied
            | Self::CalcDamage
            | Self::CalcHeal
            | Self::OnLevelUp
            | Self::OnGenerate
            | Self::OnSystem
            | Self::OnStageDone => ScriptRole::Mode,
            Self::OnThink => ScriptRole::Ai,
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_common::NamedEnum;
    use campfire_values::RelationSet;

    use super::*;

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
        for relation in [RelationSet::Enemies, RelationSet::Allies, RelationSet::All] {
            assert_eq!(RelationSet::named(relation.name()), Some(relation));
        }
        // A few as design 08 lists them.
        let think = Hook::OnThink;
        assert_eq!((think.params(), think.role()), (2, ScriptRole::Ai));
        let hit = Hook::OnHit;
        assert_eq!((hit.params(), hit.role()), (4, ScriptRole::Action));
        assert_eq!(Hook::OnModeInput.params(), 4);
    }
}
