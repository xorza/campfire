use campfire_core::{
    HookSet, ScriptBook, ScriptRole, ScriptValue, StatePlace, StateType, StateValue, SyncTo,
};
use campfire_script::ScriptId;
use campfire_values::NameTable;

use crate::mode_data::{InputType, ModeData};

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
    params: NameTable<ScriptValue>,
    state: NameTable<StateType>,
    /// Where each state field is held, in the order of their names: its class, `none` or `all`,
    /// and its place among the class's.
    places: Vec<StatePlace>,
    inputs: NameTable<InputType>,
    /// The first value of each state field, of class `none` then of class `all`, each in the
    /// order of their names.
    initial: [Vec<StateValue>; 2],
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
            hooks: scripts.defines(Some(script), ScriptRole::Mode),
            params: NameTable::default(),
            state: NameTable::default(),
            places: Vec::with_capacity(data.state.len()),
            inputs: NameTable::default(),
            initial: [Vec::new(), Vec::new()],
        };
        let params = data.params.iter();
        schema
            .params
            .push(params.map(|(name, param)| (name.as_str(), param.to_value())));
        let state = data.state.iter();
        schema
            .state
            .push(state.map(|(name, field)| (name.as_str(), field.decl.kind())));
        let mut counts = [0; 2];
        for field in data.state.values() {
            let class = usize::from(field.sync == SyncTo::All);
            schema.places.push(StatePlace {
                sync: field.sync,
                at: counts[class],
            });
            counts[class] += 1;
            schema.initial[class].push(field.decl.initial().clone());
        }
        let inputs = data.inputs.iter();
        schema
            .inputs
            .push(inputs.map(|(name, &kind)| (name.as_str(), kind)));
        schema
    }

    /// The param `name`, as `ctx.p` reads it.
    pub(crate) fn param(&self, name: &str) -> Option<ScriptValue> {
        let at = self.params.named(RUN, name)?;
        Some(self.params.values(RUN)[at].clone())
    }

    /// The state field `name`.
    pub(crate) fn state_field(&self, name: &str) -> Option<StateField> {
        let index = self.state.named(RUN, name)?;
        Some(StateField {
            index,
            kind: self.state.values(RUN)[index],
        })
    }

    /// Whether `values` hold a value of each state field of class `sync`, of its type, in the
    /// order of their names.
    pub(crate) fn fits_state(&self, sync: SyncTo, values: &[StateValue]) -> bool {
        let kinds = self.state.values(RUN).iter().zip(&self.places);
        let mut kinds = kinds.filter(|(_, place)| place.sync == sync);
        let fits = values
            .iter()
            .all(|value| kinds.next().is_some_and(|(&kind, _)| value.kind() == kind));
        fits && kinds.next().is_none()
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
        into.reserve_exact(self.places.len());
        for place in &self.places {
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
        for (value, place) in state.iter().zip(&self.places) {
            let class = if place.sync == SyncTo::All {
                &mut *all
            } else {
                &mut *none
            };
            class.push(value.clone());
        }
    }

    /// The first value of each state field of class `sync`, in the order of their names.
    pub(crate) fn initial(&self, sync: SyncTo) -> &[StateValue] {
        debug_assert!(
            matches!(sync, SyncTo::None | SyncTo::All),
            "a mode field syncs to no one or to all"
        );
        &self.initial[usize::from(sync == SyncTo::All)]
    }

    /// The input type `name`.
    pub(crate) fn input_type(&self, name: &str) -> Option<InputType> {
        let at = self.inputs.named(RUN, name)?;
        Some(self.inputs.values(RUN)[at])
    }
}
