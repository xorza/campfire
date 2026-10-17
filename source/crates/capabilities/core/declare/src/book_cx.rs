use std::any::{Any, TypeId, type_name};
use std::cell::{Cell, OnceCell};
use std::{fmt, mem};

use campfire_script::ScriptId;
use campfire_sim::{Capability, TickRate};
use campfire_values::{NameId, Names, PackageIndex, PackagePath, TypeScope, UnitType};

use crate::book::Book;
use crate::book_source::{BookSource, SourceKind, SourcePackage, SourceType};
use crate::built_books::BuiltBooks;
use crate::error::{BookError, BookPlace};
use crate::fact::Fact;
use crate::section::{Section, SectionTable};

/// How a module builds a book or a fact from what the build gives it.
pub type BuildFn<B> = fn(&BookCx<'_>) -> Result<B, BookError>;

/// How a module adds its part to a book or a fact of a module it builds on.
pub type ExtendFn<B> = fn(&BookCx<'_>, &mut B) -> Result<(), BookError>;

/// How a module checks a rule of the data it reads, which no book of its holds.
pub type RuleFn = fn(&BookCx<'_>) -> Result<(), BookError>;

/// What a match's books are built from, and the books and facts its modules declare, each built
/// the first time a builder asks for it, its extensions added, so a builder reads the books of
/// the modules it builds on, whatever their order (02-engine-core.md, Books built by their
/// owners). A book that asks again for itself, through the books it asks for, is a logic error.
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
}

/// How an extension adds its module's part to the value of its cell.
type Extension = Box<dyn Fn(&BookCx<'_>, &mut (dyn Any + Send + Sync)) -> Result<(), BookError>>;

/// One declared book or fact: how it builds, what extends it, whether the match installs it, and
/// the value once built.
struct BookCell {
    type_id: TypeId,
    name: &'static str,
    kept: Kept,
    build: Box<dyn Fn(&BookCx<'_>) -> Result<Box<dyn Any + Send + Sync>, BookError>>,
    extends: Vec<Extension>,
    building: Cell<bool>,
    value: OnceCell<Box<dyn Any + Send + Sync>>,
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
        }
    }

    pub const fn source(&self) -> &BookSource<'a> {
        &self.source
    }

    pub const fn rate(&self) -> TickRate {
        self.source.rate
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
        assert!(
            !cell.building.replace(true),
            "{} builds of itself",
            cell.name
        );
        let mut value = (cell.build)(self)?;
        for extend in &cell.extends {
            extend(self, &mut *value)?;
        }
        cell.building.set(false);
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

    fn declare<T: Send + Sync + 'static>(&mut self, kept: Kept, build: BuildFn<T>) {
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
            build: Box::new(move |cx| {
                let value: Box<dyn Any + Send + Sync> = Box::new(build(cx)?);
                Ok(value)
            }),
            extends: extends.into_iter().map(|(_, _, extend)| extend).collect(),
            building: Cell::new(false),
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
    /// time it is asked for; an error for the first a build refuses.
    pub fn build_all(&self) -> Result<(), BookError> {
        self.assert_declared();
        for at in 0..self.cells.len() {
            self.built(at)?;
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
        let value = self.cells[at].value.take().expect("the cell is built");
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
mod tests {
    use std::collections::BTreeMap;
    use std::num::NonZeroU32;

    use bevy_ecs::resource::Resource;
    use campfire_codec::{Toml, TomlTable};
    use campfire_common::DeclaredName;
    use campfire_script::View;
    use serde::Deserialize;

    use super::*;
    use crate::book_source::{Given, ScriptAt};
    use crate::capability_set::CapabilitySet;
    use crate::section_readers::SectionReaders;
    use crate::sections::Sections;
    use crate::unit_type_data::UnitTypeData;

    /// A book of a test, of a count.
    #[derive(Resource, Debug, Default, PartialEq, Eq)]
    struct Count(u32);

    impl Book for Count {
        const SCRIPTS_ONLY: bool = false;

        fn share(&self, _: &View) {}
    }

    /// A book of a test that reads `Count`.
    #[derive(Resource, Debug, Default, PartialEq, Eq)]
    struct Doubled(u32);

    impl Book for Doubled {
        const SCRIPTS_ONLY: bool = false;

        fn share(&self, _: &View) {}
    }

    /// A fact of a test.
    #[derive(Debug, PartialEq, Eq)]
    struct Base(u32);

    impl Fact for Base {}

    /// A delivery type's section of a test.
    #[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
    struct Shell {
        speed: Option<i64>,
    }

    impl Section for Shell {
        const TABLE: SectionTable = SectionTable::DeliveryType;
        const KEY: &'static str = "shell";
    }

    /// A book of a test that reads itself through `Doubled`.
    #[derive(Resource, Debug, Default)]
    struct Looped;

    impl Book for Looped {
        const SCRIPTS_ONLY: bool = false;

        fn share(&self, _: &View) {}
    }

    /// The error of a script `path` of the mode that it does not hold.
    fn broken(path: &str) -> BookError {
        BookError::Script {
            package: PackageIndex::MODE,
            path: PackagePath::parse(path).unwrap(),
        }
    }

    fn with_cx<T>(run: impl FnOnce(BookCx<'_>) -> T) -> T {
        let none = Sections::default();
        let source = BookSource {
            rate: TickRate::new(NonZeroU32::MIN),
            set: CapabilitySet::CORE,
            engine_tags: &[],
            tag_names: &[],
            mode: &none,
            map: &none,
            packages: Vec::new(),
            mode_script: None,
            given: Given::default(),
        };
        run(BookCx::new(source))
    }

    #[test]
    fn a_book_builds_once_as_first_asked_for_with_its_extensions() {
        with_cx(|mut cx| {
            // `Doubled` is declared first and asks for `Count`, built then of the fact `Base` and
            // extended by 1, by an extension declared before it, then by 2 more: 2 × (3 + 1 + 2).
            cx.declare_book::<Doubled>(|cx| Ok(Doubled(cx.book::<Count>()?.0 * 2)));
            cx.declare_extend::<Count>(|_, count| {
                count.0 += 1;
                Ok(())
            });
            cx.declare_book::<Count>(|cx| Ok(Count(cx.fact::<Base>()?.0)));
            cx.declare_fact::<Base>(|_| Ok(Base(3)));
            cx.declare_extend::<Count>(|_, count| {
                count.0 += 2;
                Ok(())
            });
            assert!(cx.book_if::<Looped>().unwrap().is_none());
            cx.build_all().unwrap();
            assert_eq!(cx.take_fact::<Base>().unwrap(), Some(Base(3)));
            let mut books = cx.into_books();
            assert_eq!(books.take::<Doubled>(), Some(Doubled(12)));
            assert_eq!(books.take::<Count>(), Some(Count(6)));
            assert!(books.is_empty(), "a fact is no book: {books:?}");
        });
        // An extension of a book of a module the match does not run does nothing.
        with_cx(|mut cx| {
            cx.declare_extend::<Count>(|_, _| panic!("Count is not built"));
            cx.declare_absent::<Count>();
            cx.check_all().unwrap();
            cx.build_all().unwrap();
            assert!(cx.book_if::<Count>().unwrap().is_none());
        });
    }

    #[test]
    fn the_first_error_a_build_or_a_rule_refuses_ends_it() {
        with_cx(|mut cx| {
            cx.declare_book::<Count>(|_| Err(broken("scripts/none.rhai")));
            cx.declare_book::<Doubled>(|cx| Ok(Doubled(cx.book::<Count>()?.0)));
            assert_eq!(cx.build_all(), Err(broken("scripts/none.rhai")));
        });
        // The rules check in their order, and the first the data breaks ends the check.
        with_cx(|mut cx| {
            cx.declare_rule(|_| Ok(()));
            cx.declare_rule(|_| Err(broken("scripts/first.rhai")));
            cx.declare_rule(|_| Err(broken("scripts/second.rhai")));
            assert_eq!(cx.check_all(), Err(broken("scripts/first.rhai")));
        });
    }

    #[test]
    #[should_panic(expected = "Count is extended, and no module of the release declares it")]
    fn an_extension_of_a_book_no_module_declares_is_a_logic_error() {
        with_cx(|mut cx| {
            cx.declare_extend::<Count>(|_, _| Ok(()));
            cx.build_all().unwrap();
        });
    }

    #[test]
    #[should_panic(expected = "Looped builds of itself")]
    fn a_book_that_asks_for_itself_is_a_logic_error() {
        with_cx(|mut cx| {
            cx.declare_book::<Looped>(|cx| {
                cx.book::<Doubled>()?;
                Ok(Looped)
            });
            cx.declare_book::<Doubled>(|cx| {
                cx.book::<Looped>()?;
                Ok(Doubled(0))
            });
            cx.build_all().unwrap();
        });
    }

    #[test]
    fn the_types_number_by_package_then_name_then_avatar_and_scripts_by_package() {
        let mut readers = SectionReaders::default();
        readers.add::<Shell>(None);
        let unit = |text: &str| {
            let raw = Toml::parse::<TomlTable>(text).unwrap();
            raw.read_with(UnitTypeData::seed(&readers)).unwrap()
        };
        let name = |text: &str| DeclaredName::new(text).unwrap();
        let mode: BTreeMap<DeclaredName, UnitTypeData> = [
            (name("tower"), unit("")),
            (name("bolt"), unit("shell = {}")),
        ]
        .into();
        let hero_units: BTreeMap<DeclaredName, UnitTypeData> = [(name("bolt"), unit(""))].into();
        let hero = unit("");
        let none = Sections::default();
        let paths = [
            PackagePath::parse("scripts/mode.rhai").unwrap(),
            PackagePath::parse("scripts/a.rhai").unwrap(),
            PackagePath::parse("scripts/b.rhai").unwrap(),
        ];
        let source = BookSource {
            rate: TickRate::new(NonZeroU32::MIN),
            set: CapabilitySet::CORE,
            engine_tags: &[],
            tag_names: &[],
            mode: &none,
            map: &none,
            packages: vec![
                SourcePackage {
                    name: "moba",
                    kind: SourceKind::Mode,
                    units: &mode,
                    content: &none,
                    scripts: vec![&paths[0]],
                },
                SourcePackage {
                    name: "hero-x",
                    kind: SourceKind::Avatar(&hero),
                    units: &hero_units,
                    content: &none,
                    scripts: vec![&paths[1], &paths[2]],
                },
            ],
            mode_script: Some(ScriptAt {
                package: PackageIndex::new(0),
                path: &paths[0],
            }),
            given: Given::default(),
        };
        let cx = BookCx::new(source);
        // The mode's types by name, `bolt` before `tower`; then the hero's `bolt`, in its own
        // scope, and its avatar, in the mode's.
        let ids: Vec<(usize, &str, TypeScope)> = (cx.types().iter())
            .map(|unit| (unit.unit_type.index(), unit.name, unit.scope))
            .collect();
        let hero_scope = TypeScope::Package(PackageIndex::new(1));
        assert_eq!(
            ids,
            [
                (0, "bolt", TypeScope::Mode),
                (1, "tower", TypeScope::Mode),
                (2, "bolt", hero_scope),
                (3, "hero-x", TypeScope::Mode),
            ]
        );
        // The mode's `bolt` delivers, so it stands not; the avatar is no declared type.
        let standing: Vec<usize> = cx.standing().map(|unit| unit.unit_type.index()).collect();
        assert_eq!(standing, [1, 2, 3]);
        assert!(cx.type_of(UnitType::new(3)).avatar());
        assert_eq!(cx.unit_type(hero_scope, "bolt"), Some(UnitType::new(2)));
        assert_eq!(
            cx.unit_type(TypeScope::Mode, "hero-x"),
            Some(UnitType::new(3))
        );
        assert_eq!(cx.unit_type(hero_scope, "tower"), None);
        // A script's id is its place among every package's, the mode's first.
        let hero_index = PackageIndex::new(1);
        assert_eq!(cx.script(hero_index, &paths[2]), Ok(ScriptId::nth(2)));
        assert_eq!(
            cx.script(hero_index, &paths[0]),
            Err(BookError::Script {
                package: hero_index,
                path: paths[0].clone(),
            })
        );
    }
}
