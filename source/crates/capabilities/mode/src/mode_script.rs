use std::rc::Rc;

use campfire_script::ScriptValue;
use campfire_values::NameTable;

use crate::game_map::GameMap;
use crate::mode_data::ModeData;
use crate::option_picks::OptionPicks;

/// The one run of the params' table.
const RUN: usize = 0;

/// The mode's values as its scripts read them: its params, each option's picked value among them,
/// and `ctx.map`. Script values, which only the match's thread of scripts holds, so the mode's
/// call part holds them and `MatchMode` does not; package data, not state.
#[derive(Debug)]
pub(crate) struct ModeScript {
    /// One run; each param as `ctx.p` reads it, converted once.
    params: NameTable<ScriptValue>,
    map: GameMap,
}

impl ModeScript {
    /// The values of `data`, its options of the values `picks` gives, and `map`.
    pub(crate) fn new(data: &ModeData, picks: &OptionPicks, map: GameMap) -> Rc<Self> {
        let params = data
            .params
            .iter()
            .map(|(name, param)| (name, param.to_value()));
        let options = picks
            .values(&data.options)
            .map(|(name, value)| (name, ScriptValue::from(value)));
        let mut table = NameTable::default();
        table.push(
            params
                .chain(options)
                .map(|(name, value)| (name.as_str(), value)),
        );
        Rc::new(Self { params: table, map })
    }

    /// The param `name`, as `ctx.p` reads it.
    pub(crate) fn param_named(&self, name: &str) -> Option<ScriptValue> {
        let at = self.params.named(RUN, name)?;
        Some(self.params.values(RUN)[at].clone())
    }

    /// `ctx.map`.
    pub(crate) fn map(&self) -> GameMap {
        self.map.clone()
    }
}
