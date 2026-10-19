use rhai::Dynamic;

use crate::scripts::script_value::{ScriptType, ScriptValue};
use campfire_values::NameKind;

/// A value a form gives, and the kind of name it is, when it is one.
pub(crate) trait ScriptOutput {
    /// The type of the value as a script holds it.
    type Value: 'static;

    /// The kind of name it is; none for a value that names nothing.
    const NAME_KIND: Option<NameKind>;

    /// The value in the engine's form.
    fn into_engine(self) -> Dynamic;
}

impl<R: ScriptType> ScriptOutput for R {
    type Value = R;
    const NAME_KIND: Option<NameKind> = None;

    fn into_engine(self) -> Dynamic {
        ScriptValue::engine(self)
    }
}
