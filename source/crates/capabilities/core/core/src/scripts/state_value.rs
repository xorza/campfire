use campfire_math::{Num, Vec3};
use campfire_sim::{Position, StableId};
use serde::{Deserialize, Serialize};

use crate::scripts::script_list::ScriptList;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::state_decl::StateType;
use crate::units::unit::Unit;
use crate::units::view::View;

/// A value of script state, of one of the declared types. Units are held by stable id, so a
/// value outlives the handles a call holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateValue {
    Int(i64),
    Num(Num),
    Bool(bool),
    Text(String),
    Entity(Option<StableId>),
    EntityList(Vec<StableId>),
    Pos(Position),
    Vec(Vec3),
}

impl StateValue {
    /// A script's value as a field of type `kind`; `None` when it is not of the type. An integer
    /// is a `Num` too, and `()` is an entity field's none.
    pub fn from_value(kind: StateType, value: &ScriptValue) -> Option<StateValue> {
        Some(match kind {
            StateType::Int => StateValue::Int(value.as_int()?),
            StateType::Num => StateValue::Num(match value.as_int() {
                Some(int) => Num::from_int(int)?,
                None => value.clone().try_cast::<Num>()?,
            }),
            StateType::Bool => StateValue::Bool(value.as_bool()?),
            StateType::String => StateValue::Text(value.clone().into_text()?.as_str().into()),
            StateType::Entity if value.is_unit() => StateValue::Entity(None),
            StateType::Entity => StateValue::Entity(Some(value.clone().try_cast::<Unit>()?.id)),
            StateType::EntityList => {
                let list = value.clone().into_list()?;
                let ids = list
                    .into_iter()
                    .map(|unit| unit.try_cast::<Unit>().map(|unit| unit.id))
                    .collect::<Option<_>>()?;
                StateValue::EntityList(ids)
            }
            StateType::Pos => StateValue::Pos(value.clone().try_cast::<Position>()?),
            StateType::Vec => StateValue::Vec(value.clone().try_cast::<Vec3>()?),
        })
    }

    /// The type of field the value is of.
    pub const fn kind(&self) -> StateType {
        match self {
            StateValue::Int(_) => StateType::Int,
            StateValue::Num(_) => StateType::Num,
            StateValue::Bool(_) => StateType::Bool,
            StateValue::Text(_) => StateType::String,
            StateValue::Entity(_) => StateType::Entity,
            StateValue::EntityList(_) => StateType::EntityList,
            StateValue::Pos(_) => StateType::Pos,
            StateValue::Vec(_) => StateType::Vec,
        }
    }

    /// The value as a script sees it: a unit the view did not read is `()`, and is left out of a
    /// list.
    pub fn to_value(&self, view: &View) -> ScriptValue {
        match self {
            StateValue::Int(value) => ScriptValue::of(*value),
            StateValue::Num(value) => ScriptValue::of(*value),
            StateValue::Bool(value) => ScriptValue::of(*value),
            StateValue::Text(text) => ScriptValue::of(ScriptText::from(text.as_str())),
            StateValue::Entity(id) => view.unit_value(*id),
            StateValue::EntityList(ids) => ScriptValue::of(
                ids.iter()
                    .filter_map(|&id| view.unit(id))
                    .map(ScriptValue::of)
                    .collect::<ScriptList>(),
            ),
            StateValue::Pos(pos) => ScriptValue::of(*pos),
            StateValue::Vec(vec) => ScriptValue::of(*vec),
        }
    }
}
