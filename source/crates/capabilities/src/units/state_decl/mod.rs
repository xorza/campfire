use std::str::FromStr;

use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use serde::de::Error;
use serde::{Deserialize, Deserializer};

use crate::units::state_value::StateValue;

/// A declared field of script state: its type, its first value and, for mode state, who it is
/// sent to. Script state is declared, never invented at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    pub kind: StateType,
    pub initial: StateValue,
    pub sync: Option<SyncTo>,
}

/// The type of a state field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateType {
    Int,
    Num,
    Bool,
    String,
    Entity,
    EntityList,
    Pos,
    Vec,
}

/// A default as data writes it: the field's type turns it into its first value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum StateDefault {
    Bool(bool),
    Int(i64),
    Text(String),
}

/// Which clients a field of mode state is sent to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncTo {
    None,
    Owner,
    Team,
    All,
}

impl StateDecl {
    /// A field of `kind` that starts at `default`, or at its type's zero without one; `None` when
    /// the default is not of the type. An entity, a list, a position or a vector takes no
    /// default.
    pub fn new(
        kind: StateType,
        default: Option<StateDefault>,
        sync: Option<SyncTo>,
    ) -> Option<StateDecl> {
        let initial = match (kind, default) {
            (StateType::Int, None) => StateValue::Int(0),
            (StateType::Num, None) => StateValue::Num(Num::ZERO),
            (StateType::Bool, None) => StateValue::Bool(false),
            (StateType::String, None) => StateValue::Text(String::new()),
            (StateType::Entity, None) => StateValue::Entity(None),
            (StateType::EntityList, None) => StateValue::EntityList(Vec::new()),
            (StateType::Pos, None) => StateValue::Pos(Position::ORIGIN),
            (StateType::Vec, None) => StateValue::Vec(Vec3::ZERO),
            (StateType::Int, Some(StateDefault::Int(value))) => StateValue::Int(value),
            (StateType::Num, Some(StateDefault::Int(value))) => {
                StateValue::Num(Num::from_int(value)?)
            }
            (StateType::Num, Some(StateDefault::Text(text))) => {
                StateValue::Num(Num::from_str(&text).ok()?)
            }
            (StateType::Bool, Some(StateDefault::Bool(value))) => StateValue::Bool(value),
            (StateType::String, Some(StateDefault::Text(text))) => StateValue::Text(text),
            _ => return None,
        };
        Some(StateDecl {
            kind,
            initial,
            sync,
        })
    }
}

/// A default not of its field's type fails to read, where the data enters.
impl<'de> Deserialize<'de> for StateDecl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<StateDecl, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(rename = "type")]
            kind: StateType,
            default: Option<StateDefault>,
            sync: Option<SyncTo>,
        }
        let Fields {
            kind,
            default,
            sync,
        } = Fields::deserialize(deserializer)?;
        StateDecl::new(kind, default, sync)
            .ok_or_else(|| D::Error::custom(format!("a default not of the type {kind:?}")))
    }
}

#[cfg(test)]
mod tests;
