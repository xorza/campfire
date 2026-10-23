use std::str::FromStr;
use std::sync::Arc;

use campfire_math::{Num, Vec3};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_value::ScriptValue;
use crate::scripts::state_type::StateType;
use crate::scripts::state_value::StateValue;
use crate::scripts::view::View;

pub(crate) mod state_field_decl;
pub(crate) mod state_schema;

/// A declared field of script state: its type, its first value, which is of its type, and for an
/// enum, its members. Script state is declared, never invented at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDecl {
    kind: StateType,
    initial: StateValue,
    /// An enum's members, in the order data gives them; empty for another type.
    members: Arc<[Arc<str>]>,
}

/// A default as data writes it: the field's type turns it into its first value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StateDefault {
    Bool(bool),
    Int(i64),
    Text(String),
}

impl StateDecl {
    /// A field of `kind` that starts at `default`, or at its type's zero without one; `None` when
    /// the default is not of the type, or for an enum, which `enumeration` makes. A unit, a list
    /// or a vector takes no default.
    pub fn new(kind: StateType, default: Option<StateDefault>) -> Option<Self> {
        let initial = match (kind, default) {
            (StateType::Int, None) => StateValue::Int(0),
            (StateType::Num, None) => StateValue::Num(Num::ZERO),
            (StateType::Bool, None) => StateValue::Bool(false),
            (StateType::String, None) => StateValue::Text(Arc::from("")),
            (StateType::Unit, None) => StateValue::Unit(None),
            (StateType::UnitList, None) => StateValue::UnitList(Arc::from([])),
            (StateType::Vec, None) => StateValue::Vec(Vec3::ZERO),
            (StateType::IntList, None) => StateValue::IntList(Arc::from([])),
            (StateType::NumList, None) => StateValue::NumList(Arc::from([])),
            (StateType::BoolList, None) => StateValue::BoolList(Arc::from([])),
            (StateType::StringList, None) => StateValue::TextList(Arc::from([])),
            (StateType::Int, Some(StateDefault::Int(value))) => StateValue::Int(value),
            (StateType::Num, Some(StateDefault::Int(value))) => {
                StateValue::Num(Num::from_int(value)?)
            }
            (StateType::Num, Some(StateDefault::Text(text))) => {
                StateValue::Num(Num::from_str(&text).ok()?)
            }
            (StateType::Bool, Some(StateDefault::Bool(value))) => StateValue::Bool(value),
            (StateType::String, Some(StateDefault::Text(text))) => {
                StateValue::Text(Arc::from(text))
            }
            _ => return None,
        };
        Some(Self {
            kind,
            initial,
            members: Arc::from([]),
        })
    }

    /// An enum field of `members`, which starts at `default`, or at its first member without one;
    /// `None` with no member, a member given twice, or a default that is no member.
    pub fn enumeration(members: Vec<String>, default: Option<StateDefault>) -> Option<Self> {
        let members: Arc<[Arc<str>]> = members.into_iter().map(Arc::from).collect();
        let unique = members
            .iter()
            .enumerate()
            .all(|(at, member)| !members[..at].contains(member));
        let initial = match default {
            None if members.is_empty() => return None,
            None => 0,
            Some(StateDefault::Text(text)) => {
                members.iter().position(|member| **member == *text)?
            }
            Some(StateDefault::Int(_) | StateDefault::Bool(_)) => return None,
        };
        let fits = u8::try_from(members.len()).is_ok();
        (unique && fits).then_some(Self {
            kind: StateType::Enum,
            initial: StateValue::Member(u8::try_from(initial).expect("the members fit u8")),
            members,
        })
    }

    pub const fn kind(&self) -> StateType {
        self.kind
    }

    pub const fn initial(&self) -> &StateValue {
        &self.initial
    }

    /// A script's value as a value of the field; an error when it is not of the field's type, or
    /// for an enum, a text that is no member.
    pub fn take(&self, value: &ScriptValue) -> Checked<StateValue> {
        if self.kind != StateType::Enum {
            return StateValue::from_value(self.kind, value);
        }
        let text = value
            .clone()
            .into_text()
            .ok_or_else(|| ApiError::WrongStateType.fail())?;
        let member = self
            .members
            .iter()
            .position(|member| **member == *text.as_str());
        member
            .map(|at| StateValue::Member(u8::try_from(at).expect("the members fit u8")))
            .ok_or_else(|| ApiError::NotAMember.fail().into())
    }

    /// Whether `value` is a value of the field: of its type, and for an enum, a member.
    pub fn holds(&self, value: &StateValue) -> bool {
        match value {
            StateValue::Member(at) => usize::from(*at) < self.members.len(),
            value => value.kind() == self.kind,
        }
    }

    /// `value`, a value of the field, as a script sees it: an enum's member as its text, which
    /// `view` keeps for each enum, and any other as `StateValue::to_value` gives it.
    pub fn read(&self, value: &StateValue, view: &View) -> ScriptValue {
        match value {
            StateValue::Member(at) => ScriptValue::of(view.member_text(&self.members, *at)),
            value => value.to_value(view),
        }
    }

    /// The text of member `at` of the field, an enum.
    pub fn member(&self, at: u8) -> &str {
        &self.members[usize::from(at)]
    }

    /// `new`, or for an enum `enumeration` of `members`, as a read takes it: members of another
    /// type, or a default not of the type, fail it.
    pub fn of<E: Error>(
        kind: StateType,
        default: Option<StateDefault>,
        members: Option<Vec<String>>,
    ) -> Result<Self, E> {
        let decl = match (kind, members) {
            (StateType::Enum, Some(members)) => Self::enumeration(members, default),
            (StateType::Enum, None) => return Err(E::custom("an enum with no `values`")),
            (_, Some(_)) => return Err(E::custom(format!("`values` of the type {kind:?}"))),
            (_, None) => Self::new(kind, default),
        };
        decl.ok_or_else(|| {
            E::custom(format!(
                "a default not of the type {kind:?}, or members none or given twice"
            ))
        })
    }
}

/// A declared field as data writes it, which reads and writes the same keys.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeclFields {
    #[serde(rename = "type")]
    pub(crate) kind: StateType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) default: Option<StateDefault>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) values: Option<Vec<String>>,
}

impl StateDecl {
    /// Its fields as data writes them: its first value as its default, but of a type that takes
    /// none, and an enum's members, which `of` reads back to it.
    pub(crate) fn fields(&self) -> DeclFields {
        let default = match &self.initial {
            StateValue::Int(value) => Some(StateDefault::Int(*value)),
            StateValue::Num(value) => Some(StateDefault::Text(value.to_string())),
            StateValue::Bool(value) => Some(StateDefault::Bool(*value)),
            StateValue::Text(text) => Some(StateDefault::Text(text.to_string())),
            StateValue::Member(at) => Some(StateDefault::Text(self.member(*at).to_owned())),
            StateValue::Unit(_)
            | StateValue::UnitList(_)
            | StateValue::Vec(_)
            | StateValue::IntList(_)
            | StateValue::NumList(_)
            | StateValue::BoolList(_)
            | StateValue::TextList(_) => None,
        };
        let values = (self.kind == StateType::Enum)
            .then(|| self.members.iter().map(ToString::to_string).collect());
        DeclFields {
            kind: self.kind,
            default,
            values,
        }
    }
}

impl Serialize for StateDecl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.fields().serialize(serializer)
    }
}

/// A default not of its field's type fails to read, where the data enters.
impl<'de> Deserialize<'de> for StateDecl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let DeclFields {
            kind,
            default,
            values,
        } = DeclFields::deserialize(deserializer)?;
        Self::of::<D::Error>(kind, default, values)
    }
}

#[cfg(test)]
mod tests;
