use campfire_common::EnumAll;
use serde::{Deserialize, Serialize};

/// Why the import drops an upgrade module, or an object's sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum UpgradeDropCause {
    /// A module of an effect other than an armor set, a weapon set, a weapon bonus, the most
    /// life, a model condition or sub-objects, by its object's name and its class's.
    Effect,
    /// A `ModelConditionUpgrade` of no `ConditionFlag`, which sets none.
    NoCondition,
    /// A `MaxHealthUpgrade` of a `ChangeType` other than `ADD_CURRENT_HEALTH_TOO`, which the
    /// engine's rising maximum does (04-capabilities/stats.md, A rising maximum raises the amount
    /// by as much, and a falling one keeps it).
    HealthChange,
    /// A module of no trigger, or of one no file defines.
    NoUpgrade,
    /// A `MaxHealthUpgrade` whose `AddMaxHealth` is no whole number above 0.
    BadNumber,
    /// A module of a fired projectile's type, which holds its flight alone (09-zero-hour.md, The
    /// import converts shells, splash, scatter, pitch, turrets and slow weapons).
    Projectile,
    /// The name of a trigger's or a conflict's modifier, or of the effect's, passes the bytes a
    /// declared name holds.
    LongName,
    /// An object holds a weapon set or an armor set of a condition other than none or
    /// `PLAYER_UPGRADE` alone, which it neither starts with nor takes by an upgrade, by the
    /// object's name.
    Set,
}
