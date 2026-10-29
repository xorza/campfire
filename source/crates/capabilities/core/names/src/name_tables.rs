use std::any::TypeId;
use std::fmt;
use std::sync::Arc;

use campfire_values::{MatchScope, NameId, NameKey, NameKind, Named, Names};

use crate::typed_list::{TypedItem, TypedList};

/// The names of a match's ids, one table for each kind of id, found by the id's type: what the
/// load resolves the packages' names by, each module adding the tables of its own ids, and what
/// the match's `MatchNames` holds for its scripts.
#[derive(Debug, Default)]
pub struct NameTables(TypedList<dyn AnyTable>);

/// The names of the ids of type `I`.
#[derive(Debug)]
struct Table<I: NameId>(Arc<Names<I>>);

/// A table of the names of one kind of id, among the others.
trait AnyTable: TypedItem + fmt::Debug + Send + Sync {
    /// The type of its ids.
    fn id_type(&self) -> TypeId;

    /// Each name, by its id's place.
    fn texts(&self) -> Box<dyn Iterator<Item = &str> + '_>;

    fn clone_table(&self) -> Box<dyn AnyTable>;

    fn kind(&self) -> NameKind;

    /// Whether the table holds `name` in the scope of the match's own names; none for a kind
    /// whose names are each a package's own.
    fn has(&self, name: &str) -> Option<bool>;
}

/// The names of the ids of one kind, by their places, as `NameTables::texts` gives them.
pub struct KindTexts<'a> {
    /// The type of the ids.
    pub id_type: TypeId,
    pub texts: Box<dyn Iterator<Item = &'a str> + 'a>,
}

/// Its kind, as the names stream.
impl fmt::Debug for KindTexts<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KindTexts")
            .field("id_type", &self.id_type)
            .finish_non_exhaustive()
    }
}

impl<I: NameId + Named + 'static> AnyTable for Table<I>
where
    Names<I>: Send + Sync,
    I::Scope: MatchScope,
{
    fn id_type(&self) -> TypeId {
        TypeId::of::<I>()
    }

    fn texts(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.0.texts())
    }

    fn clone_table(&self) -> Box<dyn AnyTable> {
        Box::new(Self(Arc::clone(&self.0)))
    }

    fn kind(&self) -> NameKind {
        I::NAMES
    }

    fn has(&self, name: &str) -> Option<bool> {
        let scope = I::Scope::MATCH?;
        Some(self.0.named((scope, name)).is_some())
    }
}

impl Clone for NameTables {
    fn clone(&self) -> Self {
        let mut tables = TypedList::default();
        for table in &self.0 {
            tables.add(table.clone_table());
        }
        Self(tables)
    }
}

impl NameTables {
    /// Puts in the names of the ids of type `I`, in place of those it held.
    pub fn set<I: NameId + Named + 'static>(&mut self, names: Arc<Names<I>>)
    where
        Names<I>: Send + Sync,
        I::Scope: MatchScope,
    {
        match self.0.get_mut::<Table<I>>() {
            Some(held) => *held = Table(names),
            None => {
                self.0.add(Box::new(Table(names)));
            }
        }
    }

    /// The names of the ids of type `I`; none before their module puts them in.
    pub fn table<I: NameId + 'static>(&self) -> Option<&Arc<Names<I>>> {
        Some(&self.0.get::<Table<I>>()?.0)
    }

    /// The id of type `I` named `key`.
    pub fn named<I: NameId + 'static>(&self, key: impl NameKey<I::Scope>) -> Option<I> {
        self.table::<I>()?.named(key)
    }

    /// Whether a table of kind `kind` holds `name` in the scope of the match's own names: none
    /// for a kind it holds no table of, an inner none for a kind whose names are each a
    /// package's own.
    pub fn has(&self, kind: NameKind, name: &str) -> Option<Option<bool>> {
        let table = self.0.iter().find(|table| table.kind() == kind)?;
        Some(table.has(name))
    }

    /// Each table's names, by the type of its ids, in the order they were put in, as the script
    /// view makes its strings of them.
    pub fn texts(&self) -> impl Iterator<Item = KindTexts<'_>> {
        self.0.iter().map(|table| KindTexts {
            id_type: table.id_type(),
            texts: table.texts(),
        })
    }
}
