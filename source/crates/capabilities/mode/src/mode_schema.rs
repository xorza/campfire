use campfire_script::{HookSet, ScriptId, ScriptRole, StateSchema, StateValue, SyncTo};
use campfire_units::ScriptBook;
use campfire_values::NameTable;

use crate::mode_data::{InputType, ModeData};

/// The one run of each of the schema's tables.
const RUN: usize = 0;

/// The mode's script and what its data declares: the hooks the script defines, the state fields
/// and the input types. Package data, not state.
#[derive(Debug)]
pub(crate) struct ModeSchema {
    pub(crate) script: ScriptId,
    /// Which of the mode's hooks the engine calls its script defines.
    pub(crate) hooks: HookSet,
    /// The state's fields, each of class `none` or `all`.
    pub(crate) state: StateSchema,
    inputs: NameTable<InputType>,
}

impl ModeSchema {
    /// The schema of `data`, whose script is `script`, which defines the hooks `scripts` gives.
    pub(crate) fn new(script: ScriptId, scripts: &ScriptBook, data: &ModeData) -> Self {
        let mut schema = Self {
            script,
            hooks: scripts.defines(Some(script), ScriptRole::Mode),
            state: StateSchema::new(
                data.state
                    .iter()
                    .map(|(name, field)| (name.as_str(), field.sync, &field.decl)),
            ),
            inputs: NameTable::default(),
        };
        let inputs = data.inputs.iter();
        schema
            .inputs
            .push(inputs.map(|(name, &kind)| (name.as_str(), kind)));
        schema
    }

    /// Fills `into` with every state field, in the order of their names, from the fields of
    /// class `none`, `none`, and of class `all`, `all`.
    pub(crate) fn merge_state(
        &self,
        none: &[StateValue],
        all: &[StateValue],
        into: &mut Vec<StateValue>,
    ) {
        into.clear();
        into.reserve_exact(self.state.len());
        for place in self.state.places() {
            let class = if place.sync == SyncTo::All { all } else { none };
            into.push(class[place.at].clone());
        }
    }

    /// Fills `none` and `all` with the fields of each class of `state`, every field in the order
    /// of their names, as `merge_state` merges them.
    pub(crate) fn split_state(
        &self,
        state: &[StateValue],
        none: &mut Vec<StateValue>,
        all: &mut Vec<StateValue>,
    ) {
        none.clear();
        all.clear();
        for (value, place) in state.iter().zip(self.state.places()) {
            let class = if place.sync == SyncTo::All {
                &mut *all
            } else {
                &mut *none
            };
            class.push(value.clone());
        }
    }

    /// The input type `name`.
    pub(crate) fn input_type_named(&self, name: &str) -> Option<InputType> {
        let at = self.inputs.named(RUN, name)?;
        Some(self.inputs.values(RUN)[at])
    }
}
