use rhai::{Dynamic, Map};

use crate::scripts::script_value::ScriptValue;

/// A table a script reads, of values by name.
#[derive(Debug, Clone)]
pub struct ScriptMap(Map);

impl ScriptMap {
    pub(crate) const fn of_engine(map: Map) -> Self {
        Self(map)
    }

    /// The table in the engine's form.
    pub(crate) fn into_engine(self) -> Dynamic {
        Dynamic::from_map(self.0)
    }
}

impl<'a> FromIterator<(&'a str, ScriptValue)> for ScriptMap {
    fn from_iter<I: IntoIterator<Item = (&'a str, ScriptValue)>>(entries: I) -> Self {
        Self(
            entries
                .into_iter()
                .map(|(name, value)| (name.into(), value.into_engine()))
                .collect(),
        )
    }
}
