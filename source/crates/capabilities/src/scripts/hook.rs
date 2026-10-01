use campfire_sim::Capability;

/// A hook the engine calls in a script, by name: every hook of the script API, whether the
/// release calls it yet or not, so the package load checks know them all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    OnCast,
    OnChannelTick,
    OnDashEnd,
    OnProjectileHit,
    OnProjectileEnd,
    OnAreaTrigger,
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
    Think,
}

/// What a script serves, as the data that names it says: each role has its own hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScriptRole {
    Ability,
    Modifier,
    Mode,
    Ai,
}

impl ScriptRole {
    pub const ALL: [ScriptRole; 4] = [
        ScriptRole::Ability,
        ScriptRole::Modifier,
        ScriptRole::Mode,
        ScriptRole::Ai,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            ScriptRole::Ability => "ability",
            ScriptRole::Modifier => "modifier",
            ScriptRole::Mode => "mode",
            ScriptRole::Ai => "AI",
        }
    }
}

impl Hook {
    pub const ALL: [Hook; 20] = [
        Hook::OnCast,
        Hook::OnChannelTick,
        Hook::OnDashEnd,
        Hook::OnProjectileHit,
        Hook::OnProjectileEnd,
        Hook::OnAreaTrigger,
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
        Hook::Think,
    ];

    /// The hook named `name`, of any role.
    pub fn named(name: &str) -> Option<Hook> {
        Hook::ALL.into_iter().find(|hook| hook.name() == name)
    }

    /// The script function the engine calls.
    pub const fn name(self) -> &'static str {
        match self {
            Hook::OnCast => "on_cast",
            Hook::OnChannelTick => "on_channel_tick",
            Hook::OnDashEnd => "on_dash_end",
            Hook::OnProjectileHit => "on_projectile_hit",
            Hook::OnProjectileEnd => "on_projectile_end",
            Hook::OnAreaTrigger => "on_area_trigger",
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
            Hook::Think => "think",
        }
    }

    /// How many parameters the function takes, `ctx` first.
    pub const fn params(self) -> usize {
        match self {
            Hook::OnMatchStart => 1,
            Hook::OnChannelTick
            | Hook::OnProjectileEnd
            | Hook::OnInterval
            | Hook::OnPlayerJoin
            | Hook::OnPlayerLeave
            | Hook::CalcDamage
            | Hook::Think => 2,
            Hook::OnCast
            | Hook::OnDashEnd
            | Hook::OnProjectileHit
            | Hook::OnAreaTrigger
            | Hook::OnAttack
            | Hook::OnAttackHit
            | Hook::OnDamageTaken
            | Hook::OnKill
            | Hook::OnTakedown
            | Hook::OnTimer => 3,
            Hook::OnModeInput | Hook::OnUnitDied => 4,
        }
    }

    pub const fn role(self) -> ScriptRole {
        match self {
            Hook::OnCast
            | Hook::OnChannelTick
            | Hook::OnDashEnd
            | Hook::OnProjectileHit
            | Hook::OnProjectileEnd
            | Hook::OnAreaTrigger => ScriptRole::Ability,
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
            | Hook::CalcDamage => ScriptRole::Mode,
            Hook::Think => ScriptRole::Ai,
        }
    }

    /// The capability that calls it; `None` for the core's.
    pub const fn capability(self) -> Option<Capability> {
        match self {
            Hook::OnCast | Hook::OnChannelTick | Hook::OnDashEnd => Some(Capability::Abilities),
            Hook::OnProjectileHit | Hook::OnProjectileEnd => Some(Capability::Projectiles),
            Hook::OnAreaTrigger => Some(Capability::Areas),
            Hook::OnInterval => Some(Capability::Stats),
            Hook::OnAttack
            | Hook::OnAttackHit
            | Hook::OnDamageTaken
            | Hook::OnKill
            | Hook::OnTakedown
            | Hook::OnUnitDied
            | Hook::CalcDamage => Some(Capability::Combat),
            Hook::OnMatchStart
            | Hook::OnModeInput
            | Hook::OnTimer
            | Hook::OnPlayerJoin
            | Hook::OnPlayerLeave => None,
            Hook::Think => Some(Capability::Orders),
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
        for relation in [Relation::Enemies, Relation::Allies, Relation::All] {
            assert_eq!(Relation::named(relation.name()), Some(relation));
        }
        // A few as design 08 lists them.
        let think = Hook::Think;
        assert_eq!((think.params(), think.role()), (2, ScriptRole::Ai));
        assert_eq!(think.capability(), Some(Capability::Orders));
        assert_eq!(Hook::OnModeInput.params(), 4);
        assert_eq!(Hook::OnTimer.capability(), None);
    }
}
