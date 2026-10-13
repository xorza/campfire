use std::ops::Deref;
use std::slice;

use campfire_script::rhai::{Array, Dynamic};

use crate::scripts::script_value::ScriptValue;

/// A list a script reads or gives, of values of any type.
#[derive(Debug, Clone, Default)]
pub struct ScriptList(Vec<ScriptValue>);

impl ScriptList {
    /// The list of the engine's `array`. A value has the layout of the engine's, so `collect`
    /// reuses the buffer, as std's in-place iteration does.
    pub(super) fn of_engine(array: Array) -> ScriptList {
        ScriptList(array.into_iter().map(ScriptValue::of_engine).collect())
    }

    /// The list in the engine's form, its buffer reused as `of_engine`'s is.
    pub(super) fn into_engine(self) -> Dynamic {
        let array: Array = self.0.into_iter().map(ScriptValue::into_engine).collect();
        Dynamic::from_array(array)
    }
}

impl Deref for ScriptList {
    type Target = [ScriptValue];

    fn deref(&self) -> &[ScriptValue] {
        &self.0
    }
}

impl FromIterator<ScriptValue> for ScriptList {
    fn from_iter<I: IntoIterator<Item = ScriptValue>>(values: I) -> ScriptList {
        ScriptList(values.into_iter().collect())
    }
}

impl<const N: usize> From<[ScriptValue; N]> for ScriptList {
    fn from(values: [ScriptValue; N]) -> ScriptList {
        ScriptList::from_iter(values)
    }
}

impl IntoIterator for ScriptList {
    type Item = ScriptValue;
    type IntoIter = std::vec::IntoIter<ScriptValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a ScriptList {
    type Item = &'a ScriptValue;
    type IntoIter = slice::Iter<'a, ScriptValue>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
