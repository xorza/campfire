use campfire_script::rhai::{Dynamic, ImmutableString};
use campfire_script::{ScriptHost, ScriptId};

use crate::mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
use crate::scripts::hook::Hook;
use crate::scripts::state_decl::StateType;
use crate::scripts::state_value::StateValue;
use crate::values::name_table::NameTable;

/// The one run of each of the schema's tables.
const RUN: usize = 0;

/// The mode's script and what its data declares: the hooks the script defines, the params, the
/// state fields and the input types. Package data, not state.
#[derive(Debug)]
pub(crate) struct ModeSchema {
    pub(crate) script: ScriptId,
    /// Which of the mode's hooks the engine calls its script defines.
    pub(crate) on_match_start: bool,
    pub(crate) on_mode_input: bool,
    pub(crate) on_timer: bool,
    /// One run each.
    params: NameTable<ModeParam>,
    state: NameTable<StateType>,
    inputs: NameTable<InputType>,
    /// Each state field's first value, in the order of their names.
    pub(crate) state_initial: Vec<StateValue>,
}

/// A field of the mode's state: its place in `ModeState`, and its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StateField {
    pub(crate) index: usize,
    pub(crate) kind: StateType,
}

impl ModeSchema {
    /// The schema of `data`, whose script `host` compiled as `script`.
    pub(crate) fn new(script: ScriptId, host: &ScriptHost, data: &ModeData) -> ModeSchema {
        let defines = |hook: Hook| host.defines(script, hook.name(), hook.params());
        let mut schema = ModeSchema {
            script,
            on_match_start: defines(Hook::OnMatchStart),
            on_mode_input: defines(Hook::OnModeInput),
            on_timer: defines(Hook::OnTimer),
            params: NameTable::default(),
            state: NameTable::default(),
            inputs: NameTable::default(),
            state_initial: data
                .state
                .values()
                .map(|decl| decl.initial.clone())
                .collect(),
        };
        let params = data.params.iter();
        schema
            .params
            .push(params.map(|(name, param)| (name.as_str(), param.clone())));
        let state = data.state.iter();
        schema
            .state
            .push(state.map(|(name, decl)| (name.as_str(), decl.kind)));
        let inputs = data.inputs.iter();
        schema
            .inputs
            .push(inputs.map(|(name, &kind)| (name.as_str(), kind)));
        schema
    }

    /// The param `name`, as `ctx.p` reads it.
    pub(crate) fn param(&self, name: &str) -> Option<Dynamic> {
        Some(match self.params.get(RUN, name)? {
            ModeParam::Value(value) => value.to_dynamic(),
            ModeParam::List(entries) => Dynamic::from_array(
                entries
                    .iter()
                    .map(|entry| match entry {
                        ListEntry::Value(value) => value.to_dynamic(),
                        ListEntry::Text(text) => {
                            Dynamic::from(ImmutableString::from(text.as_str()))
                        }
                    })
                    .collect(),
            ),
        })
    }

    /// The state field `name`.
    pub(crate) fn state_field(&self, name: &str) -> Option<StateField> {
        let index = self.state.find(RUN, name)?;
        Some(StateField {
            index,
            kind: self.state.values(RUN)[index],
        })
    }

    pub(crate) fn input_type(&self, name: &str) -> Option<InputType> {
        self.inputs.get(RUN, name).copied()
    }
}
