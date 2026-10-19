use campfire_script::{
    Hook, HookSet, ListPlace, ListedScript, ModeScripts, ScriptId, ScriptRole, StateSchema,
    StateValue, SyncTo,
};
use campfire_units::ScriptBook;
use campfire_values::NameTable;

use crate::mode_data::{InputType, ModeData};

/// The one run of each of the schema's tables.
const RUN: usize = 0;

/// The mode's scripts and what its data declares: the hooks each script defines, the state
/// fields, the players' state fields and the input types. Package data, not state.
#[derive(Debug)]
pub(crate) struct ModeSchema {
    /// In its list's order, each with the hooks of the mode's role it defines.
    pub(crate) scripts: Box<[ListedScript]>,
    /// The place of the mode's own script, which holds the state and the choices.
    pub(crate) mode: ListPlace,
    /// Which of the mode's hooks the engine calls one of its scripts defines.
    pub(crate) hooks: HookSet,
    /// The state's fields, each of class `none` or `all`.
    pub(crate) state: StateSchema,
    /// The fields of each player's state.
    pub(crate) players: StateSchema,
    inputs: NameTable<InputType>,
}

impl ModeSchema {
    /// The schema of `data`, whose scripts are `scripts`, each defining the hooks `book` gives.
    pub(crate) fn new(scripts: &ModeScripts, book: &ScriptBook, data: &ModeData) -> Self {
        let mode = scripts.mode_place();
        let scripts: Box<[ListedScript]> = scripts
            .iter()
            .map(|script| ListedScript {
                script,
                hooks: book.defines(Some(script), ScriptRole::Mode),
            })
            .collect();
        let hooks = scripts.iter().flat_map(|listed| &listed.hooks).collect();
        let mut schema = Self {
            scripts,
            mode,
            hooks,
            state: StateSchema::new(
                data.state
                    .iter()
                    .map(|(name, field)| (name.as_str(), field.sync, &field.decl)),
            ),
            players: StateSchema::new(
                data.player_state
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

    /// The one script that defines `hook`, a pure hook, and its place: the load refuses a mode
    /// whose list holds two (03-game-scripting.md, A pure hook in one script).
    pub(crate) fn pure_script(&self, hook: Hook) -> Option<(ListPlace, ScriptId)> {
        let (at, listed) = self
            .scripts
            .iter()
            .enumerate()
            .find(|(_, listed)| listed.hooks.contains(hook))?;
        let place = ListPlace::new(at).expect("a mode's list is within its limit");
        Some((place, listed.script))
    }

    /// The input type `name`.
    pub(crate) fn input_type_named(&self, name: &str) -> Option<InputType> {
        let at = self.inputs.named(RUN, name)?;
        Some(self.inputs.values(RUN)[at])
    }
}
