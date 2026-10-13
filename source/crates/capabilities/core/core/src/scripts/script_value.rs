use std::any::{Any, TypeId};

use campfire_script::rhai::{Dynamic, Variant};
use campfire_values::Scalar;

use crate::scripts::script_list::ScriptList;
use crate::scripts::script_map::ScriptMap;
use crate::scripts::script_text::ScriptText;

/// A value a script reads or gives, of any type a script holds, so no module but `scripts` names
/// the script engine's. A list, a table or a text in it is in the engine's form, as a script
/// reads one.
#[derive(Debug, Clone, Default)]
#[repr(transparent)]
pub struct ScriptValue(Dynamic);

/// A Rust type a script holds as a value: a number, a flag, a point, a handle the API registers,
/// or a value, a list, a table or a text of `scripts`.
pub trait ScriptType: Variant + Clone {}

impl<T: Variant + Clone> ScriptType for T {}

impl ScriptValue {
    /// No value, `()`.
    pub const UNIT: ScriptValue = ScriptValue(Dynamic::UNIT);

    /// `value`, as a script holds it.
    pub fn of<T: ScriptType>(value: T) -> ScriptValue {
        ScriptValue(ScriptValue::engine(value))
    }

    /// The value as a `T`, if it is one.
    pub fn try_cast<T: ScriptType>(self) -> Option<T> {
        let value = match same::<T, ScriptValue>(self) {
            Ok(value) => return Some(value),
            Err(value) => value,
        };
        let wanted = TypeId::of::<T>();
        if wanted == TypeId::of::<ScriptList>() {
            return same(value.into_list()?).ok();
        }
        if wanted == TypeId::of::<ScriptText>() {
            return same(value.into_text()?).ok();
        }
        if wanted == TypeId::of::<ScriptMap>() {
            return same(ScriptMap::of_engine(value.0.try_cast()?)).ok();
        }
        value.0.try_cast()
    }

    pub fn is_unit(&self) -> bool {
        self.0.is_unit()
    }

    pub fn as_bool(&self) -> Option<bool> {
        self.0.as_bool().ok()
    }

    pub fn as_int(&self) -> Option<i64> {
        self.0.as_int().ok()
    }

    /// The value as a text, if it is one.
    pub fn into_text(self) -> Option<ScriptText> {
        self.0
            .into_immutable_string()
            .ok()
            .map(ScriptText::of_engine)
    }

    /// The value as a list, if it is one.
    pub fn into_list(self) -> Option<ScriptList> {
        self.0.into_array().ok().map(ScriptList::of_engine)
    }

    /// `value` in the engine's form: a value, a list, a table or a text of `scripts` as the
    /// engine's own, any other type as itself, so a script reads each as it reads the engine's.
    pub(super) fn engine<T: ScriptType>(value: T) -> Dynamic {
        let value = match same::<ScriptValue, T>(value) {
            Ok(value) => return value.0,
            Err(value) => value,
        };
        let value = match same::<ScriptList, T>(value) {
            Ok(list) => return list.into_engine(),
            Err(value) => value,
        };
        let value = match same::<ScriptMap, T>(value) {
            Ok(map) => return map.into_engine(),
            Err(value) => value,
        };
        let value = match same::<ScriptText, T>(value) {
            Ok(text) => return Dynamic::from(text.into_engine()),
            Err(value) => value,
        };
        Dynamic::from(value)
    }

    pub(super) const fn of_engine(value: Dynamic) -> ScriptValue {
        ScriptValue(value)
    }

    pub(super) fn into_engine(self) -> Dynamic {
        self.0
    }
}

/// The value as a script sees it: an integer, or a `Num`.
impl From<Scalar> for ScriptValue {
    fn from(value: Scalar) -> ScriptValue {
        match value {
            Scalar::Int(value) => ScriptValue::of(value),
            Scalar::Decimal(value) => ScriptValue::of(value),
        }
    }
}

/// `value` as a `T` when `U` is `T`, else `value` back: a move, with no allocation. The two
/// `TypeId`s are constants, so the test folds where the function is inlined.
fn same<T: 'static, U: 'static>(value: U) -> Result<T, U> {
    if TypeId::of::<T>() != TypeId::of::<U>() {
        return Err(value);
    }
    let mut value = Some(value);
    let any: &mut dyn Any = &mut value;
    let same = any.downcast_mut::<Option<T>>().expect("`U` is `T`");
    Ok(same.take().unwrap())
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_script::{Budget, ScriptHost};

    use crate::scripts::error::CallError;
    use crate::scripts::script_args::ScriptArgs;
    use crate::scripts::script_value::{ScriptType, ScriptValue};

    impl ScriptValue {
        /// What `probe` of `source` returns, called with `args` from no pool, on the view as it
        /// stands.
        pub fn probe(
            world: &mut World,
            source: &str,
            args: impl ScriptArgs,
        ) -> Result<ScriptValue, CallError> {
            let mut host = world.non_send_mut::<ScriptHost>();
            let script = host.compile(source).expect("a probe compiles");
            host.call(
                &mut Budget::new(u64::MAX),
                script,
                "probe",
                args.into_engine(),
            )
            .map(ScriptValue::of_engine)
            .map_err(CallError::from_script)
        }

        /// The value as a `T`; a value of another type is a bug of the caller, which asked its type.
        pub fn cast<T: ScriptType>(self) -> T {
            self.try_cast()
                .unwrap_or_else(|| panic!("value is no {}", std::any::type_name::<T>()))
        }
    }
}
