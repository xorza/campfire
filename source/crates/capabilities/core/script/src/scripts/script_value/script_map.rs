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

    /// The value of `name`; none for a name the table lacks.
    pub fn get(&self, name: &str) -> Option<ScriptValue> {
        self.0.get(name).cloned().map(ScriptValue::of_engine)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_gives_the_value_of_each_name_it_holds() {
        let map: ScriptMap = [("ticks", ScriptValue::of(3_i64))].into_iter().collect();
        assert_eq!(map.get("ticks").and_then(|value| value.as_int()), Some(3));
        assert!(map.get("tick").is_none());
    }
}
