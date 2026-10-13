use serde::{Deserialize, Deserializer};

use crate::scripts::state_decl::{StateDecl, StateDefault, StateType};

/// A declared field of the mode's or a unit type's script state: its type, its first value, and
/// the clients it is sent to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncedStateDecl {
    pub decl: StateDecl,
    pub sync: SyncTo,
}

/// Which clients a field of script state is sent to: none unless the field says so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTo {
    #[default]
    None,
    Owner,
    Team,
    All,
}

/// A default not of its field's type fails to read, where the data enters.
impl<'de> Deserialize<'de> for SyncedStateDecl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<SyncedStateDecl, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(rename = "type")]
            kind: StateType,
            default: Option<StateDefault>,
            #[serde(default)]
            sync: SyncTo,
        }
        let Fields {
            kind,
            default,
            sync,
        } = Fields::deserialize(deserializer)?;
        let decl = StateDecl::of::<D::Error>(kind, default)?;
        Ok(SyncedStateDecl { decl, sync })
    }
}
