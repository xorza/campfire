use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declare::Book;
use campfire_script::{ScriptId, ScriptRole, View};
use campfire_units::ScriptBook;
use campfire_values::{ActionId, NameId, Names, PackageIndex, Rank};

use crate::action::Action;
use crate::action_data::ActionData;
use crate::action_parts::ActionParts;

/// The actions a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the actions, as the
/// script view reads them.
#[derive(Resource, Debug, Clone, Default)]
pub struct ActionBook {
    #[expect(
        clippy::rc_buffer,
        reason = "the load grows the book through `Arc::make_mut`; the clones after it share it"
    )]
    actions: Arc<Vec<Action>>,
    /// Each action's name in its package.
    names: Arc<Names<ActionId>>,
}

/// An action of a package to load: its package, its name there, its data, its compiled script
/// exactly when its data names one, and its parts resolved against the match.
#[derive(Debug)]
pub struct PackageAction<'d> {
    pub package: PackageIndex,
    pub name: &'d str,
    pub data: &'d ActionData,
    pub script: Option<ScriptId>,
    pub parts: ActionParts,
}

impl ActionBook {
    /// Whether `other` is this book as one load made it, not a book of another load: the actions
    /// a row filled from it holds.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.actions, &other.actions) && Arc::ptr_eq(&self.names, &other.names)
    }

    /// Loads `action`, its hooks those `scripts` defines; see `Actions::load`.
    pub fn load(&mut self, scripts: &ScriptBook, action: PackageAction<'_>) -> ActionId {
        let PackageAction {
            package,
            name,
            data,
            script,
            parts,
        } = action;
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

    /// Whether the book holds `id` with `rank` among its ranks, as a restored value that names an
    /// action at a rank needs.
    pub fn ranked(&self, id: ActionId, rank: Rank) -> bool {
        self.get(id).is_some_and(|action| action.has_rank(rank))
    }
}

impl Book for ActionBook {
    const SCRIPTS_ONLY: bool = false;

    /// Names the actions to scripts; the actions column takes the book from the match at each
    /// read.
    fn share(&self, view: &View) {
        view.names_mut().set_table(Arc::clone(self.names()));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use campfire_values::ActionId;

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
