use std::sync::Arc;

use crate::scripts::script_text::ScriptText;
use crate::values::names::{NameId, NameKey, Names};

/// The names of one kind as scripts read them: the table, shared with the book that holds it,
/// and each id's name as a script string, built once, so a read allocates nothing.
#[derive(Debug)]
pub(crate) struct ScriptNames<I: NameId> {
    names: Arc<Names<I>>,
    text: Vec<ScriptText>,
}

impl<I: NameId> ScriptNames<I> {
    pub(crate) fn new(names: Arc<Names<I>>) -> ScriptNames<I> {
        let text = names.texts().map(ScriptText::from).collect();
        ScriptNames { names, text }
    }

    /// The table of the names.
    pub(crate) fn names(&self) -> &Names<I> {
        &self.names
    }

    /// The id named `key`.
    pub(crate) fn named(&self, key: impl NameKey<I::Scope>) -> Option<I> {
        self.names.named(key)
    }

    /// The name of `id`; none for an id past the table.
    pub(crate) fn name(&self, id: I) -> Option<ScriptText> {
        self.text.get(id.index()).cloned()
    }
}

impl<I: NameId> Default for ScriptNames<I> {
    fn default() -> ScriptNames<I> {
        ScriptNames::new(Arc::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::team::Team;

    #[test]
    fn a_name_is_found_by_its_id_and_its_id_by_the_name() {
        let names = ScriptNames::new(Arc::new(Names::from(["a", "b"])));
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
