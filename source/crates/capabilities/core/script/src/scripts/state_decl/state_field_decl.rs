use campfire_common::EnumAll;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::scripts::state_decl::{DeclFields, StateDecl, StateDefault};
use crate::scripts::state_type::StateType;

/// A declared field of the mode's or a unit type's script state: its type, its first value, and
/// the clients it is sent to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateFieldDecl {
    pub decl: StateDecl,
    pub audience: Audience,
}

/// Which clients a field of script state is sent to: none unless the field says so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, EnumAll)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    #[default]
    None,
    Owner,
    Team,
    All,
}

impl Audience {
    /// Its place among the classes, `ALL`'s: none, owner, team, all.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A declared field as data writes it, its audience beside its declaration's keys, which reads
/// and writes the same keys.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    #[serde(rename = "type")]
    kind: StateType,
    #[serde(skip_serializing_if = "Option::is_none")]
    default: Option<StateDefault>,
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<Vec<String>>,
    #[serde(default)]
    audience: Audience,
}

impl Serialize for StateFieldDecl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let DeclFields {
            kind,
            default,
            values,
        } = self.decl.fields();
        Fields {
            kind,
            default,
            values,
            audience: self.audience,
        }
        .serialize(serializer)
    }
}

/// A default not of its field's type fails to read, where the data enters.
impl<'de> Deserialize<'de> for StateFieldDecl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Fields {
            kind,
            default,
            values,
            audience,
        } = Fields::deserialize(deserializer)?;
        let decl = StateDecl::of::<D::Error>(kind, default, values)?;
        Ok(Self { decl, audience })
    }
}
