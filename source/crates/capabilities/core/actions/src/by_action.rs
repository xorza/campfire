use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_declarations::Book;
use campfire_script::MatchNames;
use campfire_values::{ActionId, NameId};

use crate::per_action::PerAction;

/// A value for some of the match's actions, by action: what a kind's capability resolves of each
/// action of its kind, as the section of its data gives it (04-capabilities/actions.md, Each kind
/// owns its section). A clone shares the values, so a reader keeps the book itself, not a copy of
/// what it holds.
#[derive(Resource, Debug, PartialEq, Eq)]
pub struct ByAction<T: Send + Sync + 'static> {
    #[expect(
        clippy::rc_buffer,
        reason = "the book grows in place as it is built, and its clones share it after"
    )]
    entries: Arc<Vec<Option<T>>>,
}

impl<T: Send + Sync + 'static> ByAction<T> {
    /// The value of `action`; none for an action of none.
    pub fn get(&self, action: ActionId) -> Option<&T> {
        self.entries.get(action.index())?.as_ref()
    }

    /// The values of the actions that have one, by action.
    pub fn values(&self) -> impl Iterator<Item = &T> {
        self.entries.iter().flatten()
    }

    /// Each action that has a value, with it, by action.
    pub fn iter(&self) -> impl Iterator<Item = (ActionId, &T)> {
        let entries = self.entries.iter().enumerate();
        entries.filter_map(|(at, value)| Some((ActionId::at(at), value.as_ref()?)))
    }
}

impl<T: Clone + Send + Sync + 'static> ByAction<T> {
    /// Gives `action` its `value`, which it has none of yet.
    pub fn set(&mut self, action: ActionId, value: T) {
        let index = action.index();
        let entries = Arc::make_mut(&mut self.entries);
        if entries.len() <= index {
            entries.resize_with(index + 1, || None);
        }
        assert!(entries[index].is_none(), "an action has one value");
        entries[index] = Some(value);
    }
}

impl<T: Send + Sync + 'static> Clone for ByAction<T> {
    fn clone(&self) -> Self {
        Self {
            entries: Arc::clone(&self.entries),
        }
    }
}

impl<T: Send + Sync + 'static> Default for ByAction<T> {
    fn default() -> Self {
        Self {
            entries: Arc::default(),
        }
    }
}

impl<T: PerAction> Book for ByAction<T> {
    const SCRIPTS_ONLY: bool = T::SCRIPTS_ONLY;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_action_gives_its_value_and_another_none() {
        let mut book = ByAction::default();
        book.set(ActionId::new(2), 'b');
        book.set(ActionId::new(0), 'a');
        assert_eq!(book.get(ActionId::new(0)), Some(&'a'));
        assert_eq!(book.get(ActionId::new(1)), None);
        assert_eq!(book.get(ActionId::new(2)), Some(&'b'));
        assert_eq!(book.get(ActionId::new(3)), None);
        assert_eq!(book.values().collect::<Vec<_>>(), [&'a', &'b']);
        let held: Vec<_> = book.iter().collect();
        assert_eq!(held, [(ActionId::new(0), &'a'), (ActionId::new(2), &'b')]);
        // A clone shares its values, and a set after it changes only the book it sets.
        let shared = book.clone();
        book.set(ActionId::new(1), 'c');
        assert_eq!(shared.get(ActionId::new(1)), None);
        assert_eq!(book.get(ActionId::new(1)), Some(&'c'));
    }
}
