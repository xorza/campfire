use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_core::{ActionId, Book, ScriptBook, ScriptNames, ScriptRole, View};
use campfire_script::ScriptId;
use campfire_sim::Capability;
use campfire_values::Names;

use crate::action::Action;
use crate::action_data::ActionData;
use crate::action_parts::ActionParts;

/// The actions a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the actions, as the
/// script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActionBook {
    actions: Arc<Vec<Action>>,
    /// Each action's name in its package.
    names: Arc<Names<ActionId>>,
}

impl ActionBook {
    /// Whether `other` is this book as one load made it, not a book of another load: the actions
    /// a row filled from it holds.
    pub(crate) fn same(&self, other: &ActionBook) -> bool {
        Arc::ptr_eq(&self.actions, &other.actions) && Arc::ptr_eq(&self.names, &other.names)
    }

    /// Loads `data` as the action `name` of `package`, with `parts` resolved against the match;
    /// see `Actions::load`.
    pub fn load(
        &mut self,
        scripts: &ScriptBook,
        package: u16,
        name: &str,
        data: &ActionData,
        script: Option<ScriptId>,
        parts: ActionParts,
    ) -> ActionId {
        let ActionParts {
            kind,
            passive,
            hold,
            aim,
            ranks,
            delivery,
        } = parts;
        assert_eq!(
            data.script.is_some(),
            script.is_some(),
            "an action has a script exactly when its data names one"
        );
        let hooks = scripts.defines(script, ScriptRole::Action);
        let id = Arc::make_mut(&mut self.names).add((package, name));
        Arc::make_mut(&mut self.actions).push(Action {
            package,
            kind,
            passive,
            hold,
            aim,
            ranks: ranks.values,
            resource_costs: ranks.resource_costs.into_boxed_slice(),
            script,
            hooks,
            delivery,
        });
        id
    }

    /// Each action's name in its package.
    pub const fn names(&self) -> &Arc<Names<ActionId>> {
        &self.names
    }

    pub fn get(&self, id: ActionId) -> Option<&Action> {
        self.actions.get(id.index())
    }
}

impl Book for ActionBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    /// Names the actions to scripts; the actions column takes the book from the match at each
    /// read.
    fn share(&self, view: &View) {
        view.names_mut().actions = ScriptNames::new(Arc::clone(self.names()));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use campfire_core::ActionId;

    use crate::action::Action;
    use crate::action_book::ActionBook;

    impl ActionBook {
        /// Adds `action`, named `name` in its package, or for its place after `name` when an
        /// action before it has that name, as a test adds several of one kind; its id.
        pub fn push(&mut self, name: &str, action: Action) -> ActionId {
            let names = Arc::make_mut(&mut self.names);
            let id = names.try_add((action.package, name)).unwrap_or_else(|_| {
                let placed = format!("{name} {}", names.len());
                names.add((action.package, placed.as_str()))
            });
            Arc::make_mut(&mut self.actions).push(action);
            id
        }
    }
}
