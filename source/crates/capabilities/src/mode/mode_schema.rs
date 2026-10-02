use campfire_script::ScriptId;
use campfire_script::rhai::Dynamic;

use crate::mode::mode_data::{InputType, ModeData};
use crate::scripts::hook::Hook;
use crate::scripts::hook_set::HookSet;
use crate::scripts::script_book::ScriptBook;
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
    pub(crate) hooks: HookSet,
    /// One run each; each param as `ctx.p` reads it, converted once.
    params: NameTable<Dynamic>,
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
    /// The schema of `data`, whose script is `script`, which defines the hooks `scripts` gives.
    pub(crate) fn new(script: ScriptId, scripts: &ScriptBook, data: &ModeData) -> ModeSchema {
        let mut schema = ModeSchema {
            script,
            hooks: scripts.defines(
                Some(script),
                &[
                    Hook::OnMatchStart,
                    Hook::OnModeInput,
                    Hook::OnTimer,
                    Hook::OnUnitDied,
                    Hook::CalcDamage,
                    Hook::CalcHeal,
                    Hook::OnLevelUp,
                ],
            ),
            params: NameTable::default(),
            state: NameTable::default(),
            inputs: NameTable::default(),
            state_initial: data
                .state
                .values()
                .map(|field| field.decl.initial.clone())
                .collect(),
        };
        let params = data.params.iter();
        schema
            .params
            .push(params.map(|(name, param)| (name.as_str(), param.to_dynamic())));
        let state = data.state.iter();
        schema
            .state
            .push(state.map(|(name, field)| (name.as_str(), field.decl.kind)));
        let inputs = data.inputs.iter();
        schema
            .inputs
            .push(inputs.map(|(name, &kind)| (name.as_str(), kind)));
        schema
    }

    /// The param `name`, as `ctx.p` reads it.
    pub(crate) fn param_named(&self, name: &str) -> Option<Dynamic> {
        self.params.get_named(RUN, name).cloned()
    }

    /// The state field `name`.
    pub(crate) fn state_field_named(&self, name: &str) -> Option<StateField> {
        let index = self.state.named(RUN, name)?;
        Some(StateField {
            index,
            kind: self.state.values(RUN)[index],
        })
    }

    /// Whether `values` hold a value of each state field's type, in the order of their names.
    pub(crate) fn fits_state(&self, values: &[StateValue]) -> bool {
        let kinds = self.state.values(RUN);
        values.len() == kinds.len()
            && values
                .iter()
                .zip(kinds)
                .all(|(value, &kind)| value.kind() == kind)
    }

    pub(crate) fn input_type_named(&self, name: &str) -> Option<InputType> {
        self.inputs.get_named(RUN, name).copied()
    }
}
