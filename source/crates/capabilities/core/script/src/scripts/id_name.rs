use std::marker::PhantomData;

use rhai::Dynamic;

use crate::scripts::script_output::ScriptOutput;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use campfire_values::{NameKind, Named};

/// The name of an id of type `I`, as a form gives it: a script reads it as text, and the load
/// checks each literal a script compares it with as a name of `I`'s kind
/// (08-script-api.md, A returned name is checked where a script compares it).
///
/// It is not `Clone`: a value that clones is a `ScriptType`, which names no kind.
#[derive(Debug)]
pub struct IdName<I> {
    text: ScriptText,
    id: PhantomData<fn() -> I>,
}

impl<I> IdName<I> {
    pub(crate) const fn new(text: ScriptText) -> Self {
        Self {
            text,
            id: PhantomData,
        }
    }
}

impl<I> From<IdName<I>> for ScriptText {
    fn from(name: IdName<I>) -> Self {
        name.text
    }
}

impl<I: Named> ScriptOutput for IdName<I> {
    type Value = ScriptText;
    const NAME_KIND: Option<NameKind> = Some(I::NAMES);

    fn into_engine(self) -> Dynamic {
        ScriptValue::engine(self.text)
    }
}

/// The name of an id of type `I`, or none, as a form gives it: `()` for none, else as `IdName`.
/// It is no `Option`, which a value that clones may be.
#[derive(Debug)]
pub struct MaybeName<I>(Option<IdName<I>>);

impl<I> From<Option<IdName<I>>> for MaybeName<I> {
    fn from(name: Option<IdName<I>>) -> Self {
        Self(name)
    }
}

impl<I: Named> ScriptOutput for MaybeName<I> {
    type Value = ScriptValue;
    const NAME_KIND: Option<NameKind> = Some(I::NAMES);

    fn into_engine(self) -> Dynamic {
        self.0.map_or(Dynamic::UNIT, IdName::into_engine)
    }
}
