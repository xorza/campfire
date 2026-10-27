use campfire_common::DeclaredName;
use serde::{Deserialize, Serialize};

use crate::keys_file::Key;
use crate::panel_icon::PanelIcon;

/// What an action plays, each an effect by its name in the effects file (02-engine-core.md, Effects
/// are a format of the engine): the effect of its shot, `fire`, at its unit's barrel, and the
/// effect where a shot of no projectile strikes, `hit`, at its target, a projectile's own look
/// naming what it plays; and its button's icon and key on the command panel (02-engine-core.md, A
/// button's look is its action's).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PanelIcon>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Key>,
}
