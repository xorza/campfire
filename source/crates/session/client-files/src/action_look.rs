use campfire_common::DeclaredName;
use serde::{Deserialize, Serialize};

/// What an action plays, each an effect by its name in the effects file (effects.md, Effects are
/// a format of the engine): the effect of its shot, `fire`, at its unit's barrel, and the effect
/// where a shot of no projectile strikes, `hit`, at its target; a projectile's own look names
/// what it plays.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit: Option<DeclaredName>,
}
