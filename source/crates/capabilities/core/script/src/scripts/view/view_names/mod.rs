use std::any::TypeId;
use std::ops::Deref;

use campfire_names::{KindTexts, MatchNames};
use campfire_values::NameId;

use crate::scripts::id_name::IdName;
use crate::scripts::script_text::ScriptText;

/// The names scripts read, as the script view derives them from the match's `MatchNames` as a
/// batch reads it: those names, which it reads through, and each id's name as a script string,
/// built once, so a read allocates nothing.
#[derive(Debug, Default)]
pub struct ViewNames {
    names: MatchNames,
    /// Each kind's names as script strings, by the type of its ids, sorted by it, each by its
    /// id's place.
    texts: Vec<(TypeId, Box<[ScriptText]>)>,
}

impl ViewNames {
    /// The names of `names`, each a script string.
    pub(crate) fn of(names: &MatchNames) -> Self {
        let mut texts: Vec<(TypeId, Box<[ScriptText]>)> = names
            .texts()
            .map(|KindTexts { id_type, texts }| (id_type, texts.map(ScriptText::from).collect()))
            .collect();
        texts.sort_unstable_by_key(|&(id_type, _)| id_type);
        Self {
            names: names.clone(),
            texts,
        }
    }

    /// The name of `id`, an id of the match: its module put in the table of its kind before it
    /// made any id of it.
    pub fn name<I: NameId + 'static>(&self, id: I) -> IdName<I> {
        let at = self
            .texts
            .binary_search_by_key(&TypeId::of::<I>(), |&(id_type, _)| id_type)
            .expect("the match names each kind of id it holds");
        let text = self.texts[at].1.get(id.index());
        IdName::new(text.expect("an id of the match is in its table").clone())
    }
}

/// The names read through the match's names they derive from.
impl Deref for ViewNames {
    type Target = MatchNames;

    fn deref(&self) -> &MatchNames {
        &self.names
    }
}

#[cfg(test)]
mod tests;
