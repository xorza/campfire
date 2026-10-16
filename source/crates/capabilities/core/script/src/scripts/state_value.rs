use std::sync::Arc;

use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::state_type::StateType;
use crate::scripts::view::View;
use crate::scripts::view::unit::Unit;

/// A value of script state, of one of the declared types. Units are held by stable id, so a
/// value outlives the handles a call holds. A text and a list are shared, so a copy of a value
/// counts a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateValue {
    Int(i64),
    Num(Num),
    Bool(bool),
    Text(Arc<str>),
    Entity(Option<StableId>),
    EntityList(Arc<[StableId]>),
    Vec(Vec3),
}

impl StateValue {
    /// A script's value as a field of type `kind`; `None` when it is not of the type. An integer
    /// is a `Num` too, and `()` is an entity field's none.
    pub fn from_value(kind: StateType, value: &ScriptValue) -> Option<Self> {
        Some(match kind {
            StateType::Int => Self::Int(value.as_int()?),
            StateType::Num => Self::Num(match value.as_int() {
                Some(int) => Num::from_int(int)?,
                None => value.clone().try_cast::<Num>()?,
            }),
            StateType::Bool => Self::Bool(value.as_bool()?),
            StateType::String => Self::Text(Arc::from(value.clone().into_text()?.as_str())),
            StateType::Entity if value.is_unit() => Self::Entity(None),
            StateType::Entity => Self::Entity(Some(value.clone().try_cast::<Unit>()?.id)),
            StateType::EntityList => {
                let list = value.clone().into_list()?;
                let ids = list
                    .into_iter()
                    .map(|unit| unit.try_cast::<Unit>().map(|unit| unit.id))
                    .collect::<Option<_>>()?;
                Self::EntityList(ids)
            }
            StateType::Vec => Self::Vec(value.clone().try_cast::<Vec3>()?),
        })
    }

    /// The type of field the value is of.
    pub const fn kind(&self) -> StateType {
        match self {
            Self::Int(_) => StateType::Int,
            Self::Num(_) => StateType::Num,
            Self::Bool(_) => StateType::Bool,
            Self::Text(_) => StateType::String,
            Self::Entity(_) => StateType::Entity,
            Self::EntityList(_) => StateType::EntityList,
            Self::Vec(_) => StateType::Vec,
        }
    }

    /// The value as a script sees it: a unit the view did not read is `()`, and is left out of a
    /// list.
    pub fn to_value(&self, view: &View) -> ScriptValue {
        match self {
            Self::Int(value) => ScriptValue::of(*value),
            Self::Num(value) => ScriptValue::of(*value),
            Self::Bool(value) => ScriptValue::of(*value),
            Self::Text(text) => ScriptValue::of(ScriptText::from(&**text)),
            Self::Entity(id) => view.unit_value(*id),
            Self::EntityList(ids) => ScriptValue::of(
                ids.iter()
                    .filter_map(|&id| view.unit(id))
                    .map(ScriptValue::of)
                    .collect::<ScriptList>(),
            ),
            Self::Vec(vec) => ScriptValue::of(*vec),
        }
    }
}
