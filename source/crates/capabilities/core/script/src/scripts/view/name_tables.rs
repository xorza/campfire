use std::fmt;
use std::sync::Arc;

use campfire_values::{MatchScope, NameId, NameKey, NameKind, Named, Names};

use crate::scripts::script_names::ScriptNames;
use crate::scripts::typed_list::{TypedItem, TypedList};

/// The names of a match's ids, one table for each kind of id, found by the id's type: what the
/// load resolves the packages' names by, each module adding the tables of its own ids, and what
/// the match's `MatchNames` holds for its scripts.
#[derive(Debug, Default)]
pub struct NameTables(TypedList<dyn AnyTable>);

/// The names of the ids of type `I`.
#[derive(Debug)]
struct Table<I: NameId>(Arc<Names<I>>);

/// A table of the names of one kind of id, among the others.
pub(crate) trait AnyTable: TypedItem + fmt::Debug + Send + Sync {
    /// The table as scripts read it, each name a script string.
    fn script_names(&self) -> Box<dyn AnyNames>;

    fn clone_table(&self) -> Box<dyn AnyTable>;

    fn kind(&self) -> NameKind;

    /// Whether the table holds `name` in the scope of the match's own names; none for a kind
    /// whose names are each a package's own.
    fn has(&self, name: &str) -> Option<bool>;
}

/// A table of the names of one kind of id as scripts read it, among the others.
pub(crate) trait AnyNames: TypedItem + fmt::Debug {}

impl<I: NameId + Named + 'static> AnyTable for Table<I>
where
    Names<I>: Send + Sync,
    I::Scope: MatchScope,
{
    fn script_names(&self) -> Box<dyn AnyNames> {
        Box::new(ScriptNames::new(Arc::clone(&self.0)))
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

impl<I: NameId + 'static> AnyNames for ScriptNames<I> {}

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

    /// Each table as scripts read it, in the order they were put in.
    pub(crate) fn script_tables(&self) -> impl Iterator<Item = Box<dyn AnyNames>> + '_ {
        self.0.iter().map(AnyTable::script_names)
    }
}
