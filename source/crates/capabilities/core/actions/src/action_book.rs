use std::slice;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::Book;
use campfire_script::{MatchNames, ScriptId, ScriptRole};
use campfire_units::ScriptBook;
use campfire_values::{ActionId, NameId, Names, PackageIndex, Rank};

use crate::action::Action;
use crate::action_data::ActionData;
use crate::action_parts::ActionParts;

/// The actions a match loaded, times in ticks and scripts compiled. Package data, not state: a
/// restore loads it from the packages, as a new match does. A clone shares the actions, as the
/// script view reads them; `ActionBookBuilder` loads it.
#[derive(Resource, Debug, Clone)]
#[component(immutable)]
pub struct ActionBook {
    actions: Arc<[Action]>,
    /// Each action's name in its package.
    names: Arc<Names<ActionId>>,
    /// The actions of each name, in every package that names one so, by name, then by id.
    by_name: Arc<[NamedActions]>,
}

/// The actions of one name, among every package's, by id.
#[derive(Debug)]
struct NamedActions {
    name: Box<str>,
    actions: Arc<[ActionId]>,
}

/// An `ActionBook` as the load fills it, action by action.
#[derive(Debug, Default)]
pub(crate) struct ActionBookBuilder {
    actions: Vec<Action>,
    names: Names<ActionId>,
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

impl ActionBookBuilder {
    /// Loads `action`, whose parts the load resolved, as the next action of the book: its hooks
    /// those `scripts` defines for its script, which its data names exactly when it has one.
    pub(crate) fn load(&mut self, scripts: &ScriptBook, action: PackageAction<'_>) -> ActionId {
        let PackageAction {
            package,
            name,
            data,
            script,
            parts,
        } = action;
        let ActionParts {
            passive,
            hold,
            aim,
            when,
            ranks,
            delivery,
            pays,
        } = parts;
        assert_eq!(
            data.keys.script.is_some(),
            script.is_some(),
            "an action has a script exactly when its data names one"
        );
        let hooks = scripts.defines(script, ScriptRole::Action);
        let id = self.names.add((package, name));
        self.actions.push(Action {
            package,
            kind: data.kind(),
            passive,
            hold,
            aim,
            when,
            ranks: ranks.values,
            resource_costs: ranks.resource_costs.into_boxed_slice(),
            script,
            hooks,
            delivery,
            pays,
            cooldown_holder: data.keys.cooldown_holder,
        });
        id
    }

    /// The book of the actions loaded.
    pub(crate) fn build(self) -> ActionBook {
        let mut named: Vec<(&str, ActionId)> =
            self.names.iter().map(|(id, name)| (name, id)).collect();
        named.sort_unstable();
        let by_name = named
            .chunk_by(|a, b| a.0 == b.0)
            .map(|run| NamedActions {
                name: run[0].0.into(),
                actions: run.iter().map(|&(_, id)| id).collect(),
            })
            .collect();
        ActionBook {
            actions: Arc::from(self.actions),
            names: Arc::new(self.names),
            by_name,
        }
    }
}

impl Default for ActionBook {
    fn default() -> Self {
        ActionBookBuilder::default().build()
    }
}

impl ActionBook {
    /// Whether `other` is this book as one load made it, not a book of another load: the actions
    /// a row filled from it holds.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.actions, &other.actions) && Arc::ptr_eq(&self.names, &other.names)
    }

    /// Each action's name in its package.
    pub const fn names(&self) -> &Arc<Names<ActionId>> {
        &self.names
    }

    /// The actions named `name` in every package that names one so, by id; none for a name no
    /// package gives an action.
    pub fn actions_named(&self, name: &str) -> Option<&Arc<[ActionId]>> {
        let at = self
            .by_name
            .binary_search_by(|named| (*named.name).cmp(name))
            .ok()?;
        Some(&self.by_name[at].actions)
    }

    pub fn get(&self, id: ActionId) -> Option<&Action> {
        self.actions.get(id.index())
    }

    /// Each action, in the order of its id.
    pub fn iter(&self) -> slice::Iter<'_, Action> {
        self.actions.iter()
    }

    /// Whether the book holds `id` with `rank` among its ranks, as a restored value that names an
    /// action at a rank needs.
    pub fn is_ranked(&self, id: ActionId, rank: Rank) -> bool {
        self.get(id).is_some_and(|action| action.has_rank(rank))
    }
}

impl<'a> IntoIterator for &'a ActionBook {
    type Item = &'a Action;
    type IntoIter = slice::Iter<'a, Action>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Book for ActionBook {
    const SCRIPTS_ONLY: bool = false;

    /// Names the actions to scripts; the actions column takes the book from the match at each
    /// read.
    fn share(&self, names: &mut MatchNames) {
        names.set_table(Arc::clone(self.names()));
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_script::Filter;
    use campfire_units::ScriptBook;
    use campfire_values::{ActionId, NameId};

    use crate::action::Action;
    use crate::action_book::{ActionBook, ActionBookBuilder, PackageAction};

    impl ActionBook {
        /// The book's actions again as a load fills them, as a test adds an action after the
        /// book is built: a clone keeps the actions it shared.
        fn reopen(&self) -> ActionBookBuilder {
            ActionBookBuilder {
                actions: self.actions.to_vec(),
                names: (*self.names).clone(),
            }
        }

        /// Loads `action` as the next action of a built book, as `ActionBookBuilder::load` does.
        pub fn load(&mut self, scripts: &ScriptBook, action: PackageAction<'_>) -> ActionId {
            let mut book = self.reopen();
            let id = book.load(scripts, action);
            *self = book.build();
            id
        }

        /// Adds `action`, named `name` in its package, or for its place after `name` when an
        /// action before it has that name, as a test adds several of one kind; its id.
        pub fn push(&mut self, name: &str, action: Action) -> ActionId {
            let mut book = self.reopen();
            let names = &mut book.names;
            let id = names.try_add((action.package, name)).unwrap_or_else(|_| {
                let placed = format!("{name} {}", names.len());
                names.add((action.package, placed.as_str()))
            });
            book.actions.push(action);
            *self = book.build();
            id
        }

        /// Gives the action `id` the tag test `when`, as its data's `when` would.
        pub fn set_when(&mut self, id: ActionId, when: Option<Filter>) {
            let mut book = self.reopen();
            book.actions[id.index()].when = when;
            *self = book.build();
        }
    }
}
