use std::sync::Arc;

use campfire_values::{NameId, NameKey, Names};

use crate::scripts::script_text::ScriptText;

/// The names of one kind as scripts read them: the table, shared with the book that holds it,
/// and each id's name as a script string, built once, so a read allocates nothing.
#[derive(Debug)]
pub struct ScriptNames<I: NameId> {
    names: Arc<Names<I>>,
    text: Vec<ScriptText>,
}

impl<I: NameId> ScriptNames<I> {
    pub fn new(names: Arc<Names<I>>) -> Self {
        let text = names.texts().map(ScriptText::from).collect();
        Self { names, text }
    }

    /// The table of the names.
    pub fn names(&self) -> &Names<I> {
        &self.names
    }

    /// The id named `key`.
    pub fn named(&self, key: impl NameKey<I::Scope>) -> Option<I> {
        self.names.named(key)
    }

    /// The name of `id`; none for an id past the table.
    pub fn name(&self, id: I) -> Option<ScriptText> {
        self.text.get(id.index()).cloned()
    }
}

impl<I: NameId> Default for ScriptNames<I> {
    fn default() -> Self {
        Self::new(Arc::default())
    }
}

#[cfg(test)]
mod tests {
    use campfire_values::Team;

    use super::*;

    #[test]
    fn a_name_is_found_by_its_id_and_its_id_by_the_name() {
        let names = ScriptNames::new(Arc::new(Names::unique(["a", "b"])));
        let [a, b] = [0, 1].map(Team::new);
        assert_eq!(
            (names.name(b), names.name(Team::new(2))),
            (Some("b".into()), None)
        );
        assert_eq!((names.named("a"), names.named("c")), (Some(a), None));
        assert_eq!(names.names().len(), 2);
        assert_eq!(ScriptNames::<Team>::default().name(a), None);
    }
}
