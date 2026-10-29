use std::any::{Any, TypeId, type_name};
use std::cell::{Cell, OnceCell};
use std::{fmt, mem};

use std::sync::Arc;

use campfire_script::ScriptId;

use campfire_names::NameTables;
use campfire_sim::{Capability, MatchRate};
use campfire_values::{
    MatchScope, NameId, Named, Names, PackageIndex, PackagePath, ScriptSource, TypeScope, UnitType,
};

use crate::book::Book;
use crate::book_source::{BookSource, SourceKind, SourcePackage, SourceType};
use crate::built_books::BuiltBooks;
use crate::error::{BookError, BookPlace};
use crate::fact::Fact;
use crate::section::{Section, SectionTable};

/// How a module builds a book or a fact from what the build gives it.
pub type BuildFn<B> = fn(&BookCx<'_>) -> Result<B, BookError>;

/// How a module builds the names of its ids of type `I` from what the build gives it, shared
/// with a book of its that holds them.
pub type NamesFn<I> = fn(&BookCx<'_>) -> Result<Arc<Names<I>>, BookError>;

/// How a module adds its part to a book or a fact of a module it builds on.
pub type ExtendFn<B> = fn(&BookCx<'_>, &mut B) -> Result<(), BookError>;

/// How a module checks a rule of the data it reads, which no book of its holds.
pub type RuleFn = fn(&BookCx<'_>) -> Result<(), BookError>;

/// What a match's books are built from, and the books and facts its modules declare, each built
/// the first time a builder asks for it, its extensions added, so a builder reads the books of
/// the modules it builds on, whatever their order (02-engine-core.md, Books built by their
/// modules). A book that asks again for itself, through the books it asks for, is a logic error.
pub struct BookCx<'a> {
    source: BookSource<'a>,
    types: Vec<SourceType<'a>>,
    type_names: Names<UnitType>,
    /// Where each package's scripts start among the match's.
    script_starts: Vec<usize>,
    cells: Vec<BookCell>,
    /// The extensions of books and facts no module declared yet, each by the type it extends.
    pending: Vec<(TypeId, &'static str, Extension)>,
    /// The books and facts of the release's modules that the match does not run, whose
    /// extensions do nothing.
    absent: Vec<TypeId>,
    rules: Vec<RuleFn>,
    /// The cells that hold a table of names, each of one kind of id.
    name_cells: Vec<NameCell>,
}

/// A cell that holds the names of one kind of id, and how its table joins the others: none for
/// a kind of a module the match does not run, whose table is empty.
struct NameCell {
    type_id: TypeId,
    add: fn(Option<&(dyn Any + Send + Sync)>, &mut NameTables),
}

/// How a cell builds its value.
type CellBuild = Box<dyn Fn(&BookCx<'_>) -> Result<Box<dyn Any + Send + Sync>, BookError>>;

/// How an extension adds its module's part to the value of its cell.
type Extension = Box<dyn Fn(&BookCx<'_>, &mut (dyn Any + Send + Sync)) -> Result<(), BookError>>;

/// One declared book or fact: how it builds, what extends it, whether the match installs it, and
/// the value once built.
struct BookCell {
    type_id: TypeId,
    name: &'static str,
    kept: Kept,
    build: CellBuild,
    extends: Vec<Extension>,
    phase: Cell<Phase>,
    value: OnceCell<Box<dyn Any + Send + Sync>>,
}

/// Where a cell's build stands, but once it holds its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Not built, or refused, so a later ask builds it again and returns the refusal again.
    Open,
    /// Building, so an ask of itself is a cycle.
    Building,
    /// Built, and its value taken out by the load, so no later ask builds a second one.
    Taken,
}

/// What a built value is to the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kept {
    /// A book, which the install puts in.
    Book,
    /// A fact, which only the build reads.
    Fact,
}

impl<'a> BookCx<'a> {
    /// The build of `source`'s books, none declared yet; the unit types, each with its id in the
    /// order a match declares them.
    pub fn new(source: BookSource<'a>) -> Self {
        let mut types = Vec::new();
        let mut type_names = Names::default();
        let mut script_starts = Vec::with_capacity(source.packages.len());
        let mut start = 0;
        for (package, at) in source.packages.iter().zip((0..).map(PackageIndex::new)) {
            script_starts.push(start);
            start += package.scripts.len();
            let scope = TypeScope::of_package(at);
            let declared = package.units.iter().map(|(name, data)| SourceType {
                unit_type: UnitType::new(0),
                package: at,
                scope,
                name: name.as_str(),
                data,
                declared: Some(name),
            });
            let avatar = match package.kind {
                SourceKind::Avatar(data) => Some(SourceType {
                    unit_type: UnitType::new(0),
                    package: at,
                    scope: TypeScope::Mode,
                    name: package.name,
                    data,
                    declared: None,
                }),
                SourceKind::Mode | SourceKind::Loadout | SourceKind::Rules => None,
            };
            for mut unit in declared.chain(avatar) {
                // A repeat or one past the most the units' own load refuses, as it declares them.
                let Ok(unit_type) = type_names.try_add((unit.scope, unit.name)) else {
                    continue;
                };
                unit.unit_type = unit_type;
                types.push(unit);
            }
        }
        Self {
            source,
            types,
            type_names,
            script_starts,
            cells: Vec::new(),
            pending: Vec::new(),
            absent: Vec::new(),
            rules: Vec::new(),
            name_cells: Vec::new(),
        }
    }

    pub const fn source(&self) -> &BookSource<'a> {
        &self.source
    }

    pub const fn rate(&self) -> MatchRate {
        self.source.tick_rate
    }

    /// Whether the match declares `capability`; an error, of data at `at` of the package at
    /// `package` that uses it, when it does not.
    pub fn require(
        &self,
        capability: Capability,
        package: PackageIndex,
        at: impl FnOnce() -> BookPlace,
    ) -> Result<(), BookError> {
        if self.source.set.contains(capability) {
            return Ok(());
        }
        Err(BookError::Undeclared {
            package,
            at: at(),
            capability,
        })
    }

    /// The mode's section `S`, if it gives it.
    pub fn mode_section<S: Section>(&self) -> Option<&'a S> {
        debug_assert_eq!(S::TABLE, SectionTable::Mode, "{}", S::KEY);
        self.source.mode.get()
    }

    /// The map's section `S`, if it gives it.
    pub fn map_section<S: Section>(&self) -> Option<&'a S> {
        debug_assert_eq!(S::TABLE, SectionTable::Map, "{}", S::KEY);
        self.source.map.get()
    }

    /// The value of type `T` the load gives, if it gives one.
    pub fn given<T: 'static>(&self) -> Option<&'a T> {
        self.source.given.get()
    }

    /// Each package, with its place among the match's.
    pub fn packages(&self) -> impl Iterator<Item = (PackageIndex, &SourcePackage<'a>)> {
        (0..).map(PackageIndex::new).zip(&self.source.packages)
    }

    pub fn package(&self, index: PackageIndex) -> &SourcePackage<'a> {
        &self.source.packages[usize::from(index)]
    }

    /// Every unit type of the match, in the order of its ids.
    pub fn types(&self) -> &[SourceType<'a>] {
        &self.types
    }

    /// The unit types that stand, in the order of their ids: no delivery type.
    pub fn standing(&self) -> impl Iterator<Item = &SourceType<'a>> {
        self.types.iter().filter(|unit| unit.stands())
    }

    /// The type of `name` in `scope`, if the match declares one.
    pub fn unit_type(&self, scope: TypeScope, name: &str) -> Option<UnitType> {
        self.type_names.named((scope, name))
    }

    /// The type of the id `unit_type`.
    pub fn type_of(&self, unit_type: UnitType) -> &SourceType<'a> {
        &self.types[unit_type.index()]
    }

    /// The script at `path` of the package at `package`, by its place in the order a match
    /// compiles its scripts; an error for a path the package does not hold.
    pub fn script(&self, package: PackageIndex, path: &PackagePath) -> Result<ScriptId, BookError> {
        let scripts = &self.package(package).scripts;
        let at = scripts.iter().position(|held| *held == path);
        let at = at.ok_or_else(|| BookError::Script {
            package,
            path: path.clone(),
        })?;
        Ok(ScriptId::nth(self.script_starts[usize::from(package)] + at))
    }

    /// The package of the script `source`, which data of the package at `package` names: that
    /// package for its own, or the mode's rules package of its name (03-game-scripting.md, A unit
    /// type's AI may be a rules package's script); an error for a name of no rules package of the
    /// mode.
    pub fn script_package(
        &self,
        package: PackageIndex,
        source: &ScriptSource,
    ) -> Result<PackageIndex, BookError> {
        let Some(name) = source.package() else {
            return Ok(package);
        };
        let rules =
            |held: &SourcePackage<'_>| matches!(held.kind, SourceKind::Rules) && name == held.name;
        let found = self.packages().find(|(_, held)| rules(held));
        found
            .map(|(at, _)| at)
            .ok_or_else(|| BookError::ScriptPackage {
                package,
                name: name.clone(),
            })
    }

    /// The script `source`, which data of the package at `package` names, as `script_package`
    /// finds its package and `script` its place.
    pub fn script_of(
        &self,
        package: PackageIndex,
        source: &ScriptSource,
    ) -> Result<ScriptId, BookError> {
        self.script(self.script_package(package, source)?, source.path())
    }

    /// The book `B`, which a module of the match declares, built if it is not yet.
    pub fn book<B: Book>(&self) -> Result<&B, BookError> {
        let book = self.book_if::<B>()?;
        Ok(book.unwrap_or_else(|| panic!("{} is a book of the match", type_name::<B>())))
    }

    /// The book `B`, built if it is not yet; none when no module of the match declares it.
    pub fn book_if<B: Book>(&self) -> Result<Option<&B>, BookError> {
        self.value_if()
    }

    /// The fact `F`, which a module of the match declares, built if it is not yet.
    pub fn fact<F: Fact>(&self) -> Result<&F, BookError> {
        let fact = self.fact_if::<F>()?;
        Ok(fact.unwrap_or_else(|| panic!("{} is a fact of the match", type_name::<F>())))
    }

    /// The fact `F`, built if it is not yet; none when no module of the match declares it.
    pub fn fact_if<F: Fact>(&self) -> Result<Option<&F>, BookError> {
        self.value_if()
    }

    /// The names of the ids of type `I`, which a module of the match declares, built if they are
    /// not yet.
    pub fn names<I: NameId + 'static>(&self) -> Result<&Arc<Names<I>>, BookError> {
        let names = self.names_if::<I>()?;
        Ok(names.unwrap_or_else(|| panic!("the match names its {}", type_name::<I>())))
    }

    /// The names of the ids of type `I`, built if they are not yet; none when no module of the
    /// match declares them.
    pub fn names_if<I: NameId + 'static>(&self) -> Result<Option<&Arc<Names<I>>>, BookError> {
        self.value_if()
    }

    /// The names of every kind of id the match's modules declare, each built if it is not yet:
    /// what a load that resolves any module's names reads.
    pub fn name_tables(&self) -> Result<NameTables, BookError> {
        let mut tables = NameTables::default();
        for cell in &self.name_cells {
            let at = self
                .cells
                .iter()
                .position(|held| held.type_id == cell.type_id);
            let value = at.map(|at| self.built(at)).transpose()?;
            (cell.add)(value, &mut tables);
        }
        Ok(tables)
    }

    fn value_if<T: Any>(&self) -> Result<Option<&T>, BookError> {
        let at = self
            .cells
            .iter()
            .position(|cell| cell.type_id == TypeId::of::<T>());
        let Some(at) = at else {
            return Ok(None);
        };
        let value = self.built(at)?;
        Ok(Some(
            value.downcast_ref().expect("a cell holds its own type"),
        ))
    }

    /// The value of the cell at `at`, built and extended first if it is not yet.
    fn built(&self, at: usize) -> Result<&(dyn Any + Send + Sync), BookError> {
        let cell = &self.cells[at];
        if let Some(value) = cell.value.get() {
            return Ok(&**value);
        }
        match cell.phase.replace(Phase::Building) {
            Phase::Open => {}
            Phase::Building => panic!("{} builds of itself", cell.name),
            Phase::Taken => panic!("{} is asked for after the load took it", cell.name),
        }
        let built = (cell.build)(self).and_then(|mut value| {
            for extend in &cell.extends {
                extend(self, &mut *value)?;
            }
            Ok(value)
        });
        cell.phase.set(Phase::Open);
        let value = built?;
        Ok(&**cell.value.get_or_init(|| value))
    }

    /// Declares the book `B`, which `build` makes.
    pub fn declare_book<B: Book>(&mut self, build: BuildFn<B>) {
        self.declare(Kept::Book, build);
    }

    /// Declares the fact `F`, which `build` makes.
    pub fn declare_fact<F: Fact>(&mut self, build: BuildFn<F>) {
        self.declare(Kept::Fact, build);
    }

    /// Declares the names of the ids of type `I`, which `build` makes, as a fact.
    pub fn declare_names<I: NameId + Named + 'static>(&mut self, build: NamesFn<I>)
    where
        Names<I>: Send + Sync,
        I::Scope: MatchScope,
    {
        self.declare_cell::<Arc<Names<I>>>(
            Kept::Fact,
            Box::new(move |cx| {
                let value: Box<dyn Any + Send + Sync> = Box::new(build(cx)?);
                Ok(value)
            }),
        );
        self.name_cells.push(NameCell {
            type_id: TypeId::of::<Arc<Names<I>>>(),
            add: Self::add_names::<I>,
        });
    }

    /// Declares that a module of the release that the match does not run declares the names of
    /// the ids of type `I`, which the match then has none of.
    pub fn declare_absent_names<I: NameId + Named + 'static>(&mut self)
    where
        Names<I>: Send + Sync,
        I::Scope: MatchScope,
    {
        let type_id = TypeId::of::<Arc<Names<I>>>();
        if self.cells.iter().any(|cell| cell.type_id == type_id) {
            return;
        }
        self.declare_absent::<Arc<Names<I>>>();
        self.name_cells.push(NameCell {
            type_id,
            add: Self::add_names::<I>,
        });
    }

    /// Adds the table of `I` that `value` holds, or an empty one, to `tables`.
    fn add_names<I: NameId + Named + 'static>(
        value: Option<&(dyn Any + Send + Sync)>,
        tables: &mut NameTables,
    ) where
        Names<I>: Send + Sync,
        I::Scope: MatchScope,
    {
        let names = value.map(|value| {
            let names = value.downcast_ref::<Arc<Names<I>>>();
            Arc::clone(names.expect("a cell holds its own type"))
        });
        tables.set(names.unwrap_or_default());
    }

    fn declare<T: Send + Sync + 'static>(&mut self, kept: Kept, build: BuildFn<T>) {
        self.declare_cell::<T>(
            kept,
            Box::new(move |cx| {
                let value: Box<dyn Any + Send + Sync> = Box::new(build(cx)?);
                Ok(value)
            }),
        );
    }

    fn declare_cell<T: Send + Sync + 'static>(&mut self, kept: Kept, build: CellBuild) {
        assert!(
            !self
                .cells
                .iter()
                .any(|cell| cell.type_id == TypeId::of::<T>()),
            "{} is declared once",
            type_name::<T>()
        );
        let (extends, pending) = mem::take(&mut self.pending)
            .into_iter()
            .partition(|(type_id, _, _)| *type_id == TypeId::of::<T>());
        self.pending = pending;
        self.cells.push(BookCell {
            type_id: TypeId::of::<T>(),
            name: type_name::<T>(),
            kept,
            build,
            extends: extends.into_iter().map(|(_, _, extend)| extend).collect(),
            phase: Cell::new(Phase::Open),
            value: OnceCell::new(),
        });
    }

    /// Declares `extend`, which adds a module's part to `B`, a book or a fact a module declares,
    /// before it or after it, as soon as `B` is built.
    pub fn declare_extend<B: Send + Sync + 'static>(&mut self, extend: ExtendFn<B>) {
        let extension: Extension = Box::new(move |cx, value| {
            let value = value.downcast_mut().expect("a cell holds its own type");
            extend(cx, value)
        });
        let cell = self
            .cells
            .iter_mut()
            .find(|cell| cell.type_id == TypeId::of::<B>());
        match cell {
            Some(cell) => cell.extends.push(extension),
            None => self
                .pending
                .push((TypeId::of::<B>(), type_name::<B>(), extension)),
        }
    }

    /// Declares that a module of the release that the match does not run declares `T`, a book or
    /// a fact, so an extension of it does nothing: a module adds to what a module above it holds,
    /// whether or not the match runs that module.
    pub fn declare_absent<T: 'static>(&mut self) {
        if !self
            .cells
            .iter()
            .any(|cell| cell.type_id == TypeId::of::<T>())
        {
            self.absent.push(TypeId::of::<T>());
        }
    }

    /// Asserts that every extension's book or fact is declared, by the match or by a module it
    /// does not run, as a logic error of the declarations otherwise.
    fn assert_declared(&self) {
        let undeclared = self.pending.iter().find(|(type_id, _, _)| {
            let declared = |cell: &BookCell| cell.type_id == *type_id;
            !self.absent.contains(type_id) && !self.cells.iter().any(declared)
        });
        if let Some((_, name, _)) = undeclared {
            panic!("{name} is extended, and no module of the release declares it");
        }
    }

    /// Declares `rule`, which the build checks before any book.
    pub fn declare_rule(&mut self, rule: RuleFn) {
        self.rules.push(rule);
    }

    /// Checks every rule declared, in the order of their declarations; an error for the first the
    /// data breaks.
    pub fn check_all(&self) -> Result<(), BookError> {
        self.assert_declared();
        for rule in &self.rules {
            rule(self)?;
        }
        Ok(())
    }

    /// Builds every book and fact declared, in the order of their declarations, each the first
    /// time it is asked for, but a fact the load took, which no book reads; an error for the
    /// first a build refuses.
    pub fn build_all(&self) -> Result<(), BookError> {
        self.assert_declared();
        for (at, cell) in self.cells.iter().enumerate() {
            if cell.phase.get() != Phase::Taken {
                self.built(at)?;
            }
        }
        Ok(())
    }

    /// Takes the fact `F` out, built, as the match's load reads it beside the books; none when no
    /// module of the match declares it.
    pub fn take_fact<F: Fact>(&mut self) -> Result<Option<F>, BookError> {
        let Some(at) = self
            .cells
            .iter()
            .position(|cell| cell.type_id == TypeId::of::<F>())
        else {
            return Ok(None);
        };
        self.built(at)?;
        let cell = &mut self.cells[at];
        cell.phase.set(Phase::Taken);
        let value = cell.value.take().expect("the cell is built");
        Ok(Some(*value.downcast().expect("a cell holds its own type")))
    }

    /// The books, built, which the install puts in.
    pub fn into_books(self) -> BuiltBooks {
        let mut books = BuiltBooks::default();
        for cell in self.cells {
            if cell.kept == Kept::Book {
                let value = cell.value.into_inner().expect("each book is built");
                books.add_boxed(cell.name, value);
            }
        }
        books
    }
}

/// The cells by the names of their types.
impl fmt::Debug for BookCx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BookCx")
            .field("source", &self.source)
            .field("types", &self.types.len())
            .field(
                "cells",
                &self.cells.iter().map(|cell| cell.name).collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests;
