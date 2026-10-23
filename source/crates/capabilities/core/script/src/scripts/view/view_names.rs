use std::ops::Deref;

use campfire_values::NameId;

use crate::scripts::id_name::IdName;
use crate::scripts::script_names::ScriptNames;
use crate::scripts::typed_list::TypedList;
use crate::scripts::view::match_names::MatchNames;
use crate::scripts::view::name_tables::AnyNames;

/// The names scripts read, as the script view derives them from the match's `MatchNames` as a
/// batch reads it: those names, which it reads through, and each id's name as a script string,
/// built once, so a read allocates nothing.
#[derive(Debug, Default)]
pub struct ViewNames {
    names: MatchNames,
    /// The names of each kind of id as scripts read them, a `ScriptNames<I>` by the id's type `I`.
    texts: TypedList<dyn AnyNames>,
}

impl ViewNames {
    /// The names of `names`, each a script string.
    pub(crate) fn of(names: &MatchNames) -> Self {
        let mut texts = TypedList::default();
        for table in names.script_tables() {
            texts.add(table);
        }
        Self {
            names: names.clone(),
            texts,
        }
    }

    /// The names of the ids of type `I` as scripts read them; none before their module puts them
    /// in.
    pub fn texts<I: NameId + 'static>(&self) -> Option<&ScriptNames<I>> {
        self.texts.get::<ScriptNames<I>>()
    }

    /// The name of `id`, an id of the match: its module put in the table of its kind before it
    /// made any id of it.
    pub fn name<I: NameId + 'static>(&self, id: I) -> IdName<I> {
        let table = self
            .texts::<I>()
            .expect("the match names each kind of id it holds");
        IdName::new(table.name(id).expect("an id of the match is in its table"))
    }
}

/// The names read through the match's names they derive from.
impl Deref for ViewNames {
    type Target = MatchNames;

    fn deref(&self) -> &MatchNames {
        &self.names
    }
}
