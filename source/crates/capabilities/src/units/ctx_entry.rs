use campfire_sim::Capability;

use crate::units::hook::ScriptRole;

/// A name a script may use on `ctx`, as the script API defines it: whether the release runs it
/// yet or not, so the package load checks know every one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CtxEntry {
    pub name: &'static str,
    pub kind: CtxKind,
    /// The capability that adds it; `None` for the core's.
    pub capability: Option<Capability>,
    /// The roles whose scripts may use it; `None` for every role.
    pub role: Option<ScriptRole>,
}

/// How a script uses a name on `ctx`: reads it, as `ctx.p`, or calls it, as `ctx.damage(..)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtxKind {
    Value,
    Call,
}

impl CtxEntry {
    /// Every name of the script API's `ctx`.
    pub const ALL: &[CtxEntry] = &[
        value("p", None, None),
        value(
            "range",
            Some(Capability::Abilities),
            Some(ScriptRole::Ability),
        ),
        value(
            "charge",
            Some(Capability::Abilities),
            Some(ScriptRole::Ability),
        ),
        value(
            "origin",
            Some(Capability::Abilities),
            Some(ScriptRole::Ability),
        ),
        value("state", None, Some(ScriptRole::Mode)),
        value("map", Some(Capability::Navigation), None),
        value("teams", None, None),
        call("find", None, None),
        call("find_visible", Some(Capability::Vision), None),
        call("nearest_visible", Some(Capability::Vision), None),
        call("heroes", None, None),
        call("units_tagged", None, None),
        call("enemy_team", None, None),
        call("hero_available", None, None),
        call("chance", None, None),
        call("pick", None, None),
        call("damage", Some(Capability::Combat), None),
        call("heal", Some(Capability::Combat), None),
        call("restore", Some(Capability::Combat), None),
        call("attack_hit", Some(Capability::Combat), None),
        call("add_modifier", Some(Capability::Stats), None),
        call("remove", Some(Capability::Stats), None),
        call("stun", Some(Capability::Stats), None),
        call("slow", Some(Capability::Stats), None),
        call("knock_up", Some(Capability::Stats), None),
        call("knock_back", Some(Capability::Stats), None),
        call("dash", Some(Capability::Navigation), None),
        call("teleport", Some(Capability::Navigation), None),
        call(
            "projectile",
            Some(Capability::Projectiles),
            Some(ScriptRole::Ability),
        ),
        call("area", Some(Capability::Areas), Some(ScriptRole::Ability)),
        call("reveal", Some(Capability::Vision), None),
        call("reduce_cooldown", Some(Capability::Abilities), None),
        call("reduce_cooldowns", Some(Capability::Abilities), None),
        call("add_charge", Some(Capability::Abilities), None),
        call("add_resource", None, None),
        call("add_xp", Some(Capability::Stats), None),
        call(
            "order_attack",
            Some(Capability::Orders),
            Some(ScriptRole::Ai),
        ),
        call("order_move", Some(Capability::Orders), Some(ScriptRole::Ai)),
        call(
            "order_follow_lane",
            Some(Capability::Orders),
            Some(ScriptRole::Ai),
        ),
        call(
            "order_reset",
            Some(Capability::Orders),
            Some(ScriptRole::Ai),
        ),
        call("timer", None, Some(ScriptRole::Mode)),
        call("end", None, Some(ScriptRole::Mode)),
        call("spawn_heroes", None, Some(ScriptRole::Mode)),
        call("spawn_unit", None, Some(ScriptRole::Mode)),
        call("spawn_wave", None, Some(ScriptRole::Mode)),
        call("respawn", Some(Capability::Combat), Some(ScriptRole::Mode)),
        call("choose_hero", None, Some(ScriptRole::Mode)),
        call("choose_spells", None, Some(ScriptRole::Mode)),
    ];

    /// The entry of `name`.
    pub fn named(name: &str) -> Option<CtxEntry> {
        CtxEntry::ALL
            .iter()
            .copied()
            .find(|entry| entry.name == name)
    }
}

const fn value(
    name: &'static str,
    capability: Option<Capability>,
    role: Option<ScriptRole>,
) -> CtxEntry {
    CtxEntry {
        name,
        kind: CtxKind::Value,
        capability,
        role,
    }
}

const fn call(
    name: &'static str,
    capability: Option<Capability>,
    role: Option<ScriptRole>,
) -> CtxEntry {
    CtxEntry {
        name,
        kind: CtxKind::Call,
        capability,
        role,
    }
}
