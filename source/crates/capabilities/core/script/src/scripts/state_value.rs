use std::sync::Arc;

use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::script_host::error::{NumError, Raised};
use crate::scripts::error::{ApiError, Checked};
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::state_type::StateType;
use crate::scripts::view::View;
use crate::scripts::view::unit::Unit;

/// A value of script state, of one of the declared types; an enum's is its member's text. Units
/// are held by stable id, so a value outlives the handles a call holds. A text and a list are
/// shared, so a copy of a value counts a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateValue {
    Int(i64),
    Num(Num),
    Bool(bool),
    Text(Arc<str>),
    Unit(Option<StableId>),
    UnitList(Arc<[StableId]>),
    Vec(Vec3),
    IntList(Arc<[i64]>),
    NumList(Arc<[Num]>),
    BoolList(Arc<[bool]>),
    TextList(Arc<[Arc<str>]>),
}

impl StateValue {
    /// A script's value as a field of type `kind`; an error when it is not of the type, or is an
    /// integer beyond a `Num` for a `Num` field. An integer is a `Num` too, and `()` is a unit
    /// field's none.
    pub fn from_value(kind: StateType, value: &ScriptValue) -> Checked<Self> {
        Self::of_kind(kind, value).ok_or_else(|| {
            if kind == StateType::Num && value.as_int().is_some() {
                Raised::error(NumError::IntegerBeyondNum).into()
            } else {
                ApiError::WrongStateType.fail().into()
            }
        })
    }

    /// `value` as a field of type `kind`, when it is one, as `from_value` takes it; never for an
    /// enum, whose declaration takes its members.
    pub fn of_kind(kind: StateType, value: &ScriptValue) -> Option<Self> {
        Some(match kind {
            StateType::Int => Self::Int(value.as_int()?),
            StateType::Num => Self::Num(Self::num(value)?),
            StateType::Bool => Self::Bool(value.as_bool()?),
            StateType::String => Self::Text(Self::text(value)?),
            StateType::Unit if value.is_unit() => Self::Unit(None),
            StateType::Unit => Self::Unit(Some(value.clone().try_cast::<Unit>()?.id)),
            StateType::UnitList => Self::UnitList(Self::items(value, |unit| {
                Some(unit.try_cast::<Unit>()?.id)
            })?),
            StateType::Vec => Self::Vec(value.clone().try_cast::<Vec3>()?),
            StateType::IntList => Self::IntList(Self::items(value, |item| item.as_int())?),
            StateType::NumList => Self::NumList(Self::items(value, |item| Self::num(&item))?),
            StateType::BoolList => Self::BoolList(Self::items(value, |item| item.as_bool())?),
            StateType::StringList => Self::TextList(Self::items(value, |item| Self::text(&item))?),
            StateType::Enum => return None,
        })
    }

    /// A number, or an integer within a `Num`.
    fn num(value: &ScriptValue) -> Option<Num> {
        match value.as_int() {
            Some(int) => Num::from_int(int),
            None => value.clone().try_cast::<Num>(),
        }
    }

    fn text(value: &ScriptValue) -> Option<Arc<str>> {
        Some(Arc::from(value.clone().into_text()?.as_str()))
    }

    /// The items of `value`, a list, each as `item` takes it; `None` when one is not of its type.
    fn items<T>(
        value: &ScriptValue,
        item: impl FnMut(ScriptValue) -> Option<T>,
    ) -> Option<Arc<[T]>> {
        value.clone().into_list()?.into_iter().map(item).collect()
    }

    /// The type of field the value is of; an enum's value is a text.
    pub const fn kind(&self) -> StateType {
        match self {
            Self::Int(_) => StateType::Int,
            Self::Num(_) => StateType::Num,
            Self::Bool(_) => StateType::Bool,
            Self::Text(_) => StateType::String,
            Self::Unit(_) => StateType::Unit,
            Self::UnitList(_) => StateType::UnitList,
            Self::Vec(_) => StateType::Vec,
            Self::IntList(_) => StateType::IntList,
            Self::NumList(_) => StateType::NumList,
            Self::BoolList(_) => StateType::BoolList,
            Self::TextList(_) => StateType::StringList,
        }
    }

    /// The value as a script sees it: a unit the view did not read is `()`, and is left out of a
    /// list.
    pub fn to_value(&self, view: &View) -> ScriptValue {
        let list = |items: &mut dyn Iterator<Item = ScriptValue>| {
            ScriptValue::of(items.collect::<ScriptList>())
        };
        match self {
            Self::Int(value) => ScriptValue::of(*value),
            Self::Num(value) => ScriptValue::of(*value),
            Self::Bool(value) => ScriptValue::of(*value),
            Self::Text(text) => ScriptValue::of(ScriptText::from(&**text)),
            Self::Unit(id) => view.unit_value(*id),
            Self::UnitList(ids) => list(
                &mut ids
                    .iter()
                    .filter_map(|&id| view.unit(id))
                    .map(ScriptValue::of),
            ),
            Self::Vec(vec) => ScriptValue::of(*vec),
            Self::IntList(items) => list(&mut items.iter().map(|&item| ScriptValue::of(item))),
            Self::NumList(items) => list(&mut items.iter().map(|&item| ScriptValue::of(item))),
            Self::BoolList(items) => list(&mut items.iter().map(|&item| ScriptValue::of(item))),
            Self::TextList(items) => list(
                &mut items
                    .iter()
                    .map(|item| ScriptValue::of(ScriptText::from(&**item))),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script_host::error::ScriptError;
    use crate::scripts::error::CallError;
    use crate::scripts::error::FailureKind;

    #[test]
    fn a_value_takes_its_fields_type_and_an_integer_past_a_num_fails_as_the_number_apis() {
        let kind = |field, value: i64| {
            let error = StateValue::from_value(field, &ScriptValue::of(value)).unwrap_err();
            CallError::from_script(ScriptError::from_eval(error)).kind()
        };
        // 3 is an integer, and a `Num` too; 2³⁹ is the least integer past a `Num`.
        let three = ScriptValue::of(3_i64);
        assert_eq!(
            StateValue::from_value(StateType::Int, &three).unwrap(),
            StateValue::Int(3)
        );
        assert_eq!(
            StateValue::from_value(StateType::Num, &three).unwrap(),
            StateValue::Num(Num::from_int(3).unwrap())
        );
        assert_eq!(
            kind(StateType::Num, 1 << 39),
            FailureKind::Raised(Some(NumError::IntegerBeyondNum))
        );
        assert_eq!(
            StateValue::of_kind(StateType::Num, &ScriptValue::of(1_i64 << 39)),
            None
        );
        assert_eq!(
            kind(StateType::Bool, 1),
            FailureKind::Api(ApiError::WrongStateType)
        );
    }
}
