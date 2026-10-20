use std::marker::PhantomData;

use campfire_values::Named;
use rhai::Dynamic;

use crate::scripts::given_names::GivenNames;
use crate::scripts::script_output::ScriptOutput;
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::ScriptValue;
use crate::scripts::script_value::script_list::ScriptList;

/// The names of ids of type `I`, in order, as a form gives them: a script reads a list of texts,
/// and the load checks each literal a script finds in it, or compares an item of it with, as a
/// name of `I`'s kind (06-script-api.md, A name's kind reaches lists, hook arguments
/// and variables).
#[derive(Debug)]
pub struct IdNames<I> {
    names: ScriptList,
    id: PhantomData<fn() -> I>,
}

impl<I> IdNames<I> {
    /// The names `list` holds, each a text, as a book keeps a list of names it gives often.
    pub fn of_list(list: ScriptList) -> Self {
        debug_assert!(
            list.iter().all(|name| name.clone().into_text().is_some()),
            "a list of names holds texts"
        );
        Self {
            names: list,
            id: PhantomData,
        }
    }
}

/// No name.
impl<I> Default for IdNames<I> {
    fn default() -> Self {
        Self::of_list(ScriptList::default())
    }
}

impl<I> FromIterator<ScriptText> for IdNames<I> {
    fn from_iter<T: IntoIterator<Item = ScriptText>>(names: T) -> Self {
        Self {
            names: names.into_iter().map(ScriptValue::of).collect(),
            id: PhantomData,
        }
    }
}

impl<I: Named> ScriptOutput for IdNames<I> {
    type Value = ScriptList;
    const NAMES: Option<GivenNames> = Some(GivenNames::list(I::NAMES));

    fn into_engine(self) -> Dynamic {
        ScriptValue::engine(self.names)
    }
}
