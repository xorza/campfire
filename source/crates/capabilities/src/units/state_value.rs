use campfire_math::{Num, Vec3};
use campfire_script::rhai::{Array, Dynamic, INT, ImmutableString};
use campfire_sim::{Position, StableId};
use serde::{Deserialize, Serialize};

use crate::units::script_view::View;
use crate::units::state_decl::StateType;
use crate::units::unit::Unit;

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
    pub(crate) fn from_dynamic(kind: StateType, value: &Dynamic) -> Option<StateValue> {
        Some(match kind {
            StateType::Int => StateValue::Int(value.as_int().ok()?),
            StateType::Num => StateValue::Num(match value.as_int() {
                Ok(int) => Num::from_int(int)?,
                Err(_) => value.clone().try_cast::<Num>()?,
            }),
            StateType::Bool => StateValue::Bool(value.as_bool().ok()?),
            StateType::String => {
                StateValue::Text(value.clone().into_immutable_string().ok()?.into())
            }
            StateType::Entity if value.is_unit() => StateValue::Entity(None),
            StateType::Entity => StateValue::Entity(Some(value.clone().try_cast::<Unit>()?.id)),
            StateType::EntityList => {
                let list = value.clone().try_cast::<Array>()?;
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

    /// The value as a script sees it: a unit the view did not read is `()`, and is left out of a
    /// list.
    pub(crate) fn to_dynamic(&self, view: &View) -> Dynamic {
        match self {
            StateValue::Int(value) => Dynamic::from_int(INT::from(*value)),
            StateValue::Num(value) => Dynamic::from(*value),
            StateValue::Bool(value) => Dynamic::from_bool(*value),
            StateValue::Text(text) => Dynamic::from(ImmutableString::from(text.as_str())),
            StateValue::Entity(id) => id
                .and_then(|id| view.unit(id))
                .map_or(Dynamic::UNIT, Dynamic::from),
            StateValue::EntityList(ids) => Dynamic::from_array(
                ids.iter()
                    .filter_map(|&id| view.unit(id))
                    .map(Dynamic::from)
                    .collect(),
            ),
            StateValue::Pos(pos) => Dynamic::from(*pos),
            StateValue::Vec(vec) => Dynamic::from(*vec),
        }
    }
}
