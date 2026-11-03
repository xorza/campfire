use std::collections::BTreeMap;
use std::num::NonZeroU32;

use bevy_ecs::resource::Resource;
use campfire_codec::{Toml, TomlTable};
use campfire_common::DeclaredName;
use campfire_names::MatchNames;
use campfire_script::{ApiVersion, Status};
use serde::Deserialize;

use super::*;
use crate::book_source::{Given, ScriptAt};
use crate::capability_set::CapabilitySet;
use crate::section::section_readers::SectionReaders;
use crate::section::sections::Sections;
use crate::unit_defaults::UnitDefaults;
use crate::unit_type_data::UnitTypeData;

/// A book of a test, of a count.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
#[component(immutable)]
struct CountBook(u32);

impl Book for CountBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

/// A book of a test that reads `CountBook`.
#[derive(Resource, Debug, Default, PartialEq, Eq)]
#[component(immutable)]
struct DoubledBook(u32);

impl Book for DoubledBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
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
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

/// A book of a test that reads itself through `DoubledBook`.
#[derive(Resource, Debug, Default)]
#[component(immutable)]
struct LoopedBook;

impl Book for LoopedBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
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
        tick_rate: MatchRate::new(NonZeroU32::MIN),
        set: CapabilitySet::CORE,
        engine_tags: &[],
        mode: &none,
        map: &none,
        packages: Vec::new(),
        mode_scripts: Vec::new(),
        given: Given::default(),
    };
    run(BookCx::new(source))
}

#[test]
fn a_book_builds_once_as_first_asked_for_with_its_extensions() {
    with_cx(|mut cx| {
        // `DoubledBook` is declared first and asks for `CountBook`, built then of the fact `Base` and
        // extended by 1, by an extension declared before it, then by 2 more: 2 × (3 + 1 + 2).
        cx.declare_book::<DoubledBook>(|cx| Ok(DoubledBook(cx.book::<CountBook>()?.0 * 2)));
        cx.declare_extend::<CountBook>(|_, count| {
            count.0 += 1;
            Ok(())
        });
        cx.declare_book::<CountBook>(|cx| Ok(CountBook(cx.fact::<Base>()?.0)));
        cx.declare_fact::<Base>(|_| Ok(Base(3)));
        cx.declare_extend::<CountBook>(|_, count| {
            count.0 += 2;
            Ok(())
        });
        assert!(cx.book_if::<LoopedBook>().unwrap().is_none());
        cx.build_all().unwrap();
        assert_eq!(cx.take_fact::<Base>().unwrap(), Some(Base(3)));
        let mut books = cx.into_books();
        assert_eq!(books.take::<DoubledBook>(), Some(DoubledBook(12)));
        assert_eq!(books.take::<CountBook>(), Some(CountBook(6)));
        assert!(books.is_empty(), "a fact is no book: {books:?}");
    });
    // An extension of a book of a module the match does not run does nothing.
    with_cx(|mut cx| {
        cx.declare_extend::<CountBook>(|_, _| panic!("CountBook is not built"));
        cx.declare_absent::<CountBook>();
        cx.check_all().unwrap();
        cx.build_all().unwrap();
        assert!(cx.book_if::<CountBook>().unwrap().is_none());
    });
}

#[test]
fn the_first_error_a_build_or_a_rule_refuses_ends_it() {
    with_cx(|mut cx| {
        cx.declare_book::<CountBook>(|_| Err(broken("scripts/none.rhai")));
        cx.declare_book::<DoubledBook>(|cx| Ok(DoubledBook(cx.book::<CountBook>()?.0)));
        assert_eq!(cx.build_all(), Err(broken("scripts/none.rhai")));
        // A later ask of the refused book, or of the one that asked for it, refuses again.
        assert_eq!(cx.book::<CountBook>(), Err(broken("scripts/none.rhai")));
        assert_eq!(cx.book::<DoubledBook>(), Err(broken("scripts/none.rhai")));
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
#[should_panic(expected = "CountBook is extended, and no module of the release declares it")]
fn an_extension_of_a_book_no_module_declares_is_a_logic_error() {
    with_cx(|mut cx| {
        cx.declare_extend::<CountBook>(|_, _| Ok(()));
        cx.build_all().unwrap();
    });
}

#[test]
#[should_panic(expected = "LoopedBook builds of itself")]
fn a_book_that_asks_for_itself_is_a_logic_error() {
    with_cx(|mut cx| {
        cx.declare_book::<LoopedBook>(|cx| {
            cx.book::<DoubledBook>()?;
            Ok(LoopedBook)
        });
        cx.declare_book::<DoubledBook>(|cx| {
            cx.book::<LoopedBook>()?;
            Ok(DoubledBook(0))
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
        raw.read_with(UnitTypeData::seed(&readers, &UnitDefaults::default()))
            .unwrap()
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
        tick_rate: MatchRate::new(NonZeroU32::MIN),
        set: CapabilitySet::CORE,
        engine_tags: &[],
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
        mode_scripts: vec![ScriptAt {
            package: PackageIndex::new(0),
            path: &paths[0],
        }],
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
    assert!(cx.type_of(UnitType::new(3)).is_avatar());
    assert_eq!(cx.type_named(hero_scope, "bolt"), Some(UnitType::new(2)));
    assert_eq!(
        cx.type_named(TypeScope::Mode, "hero-x"),
        Some(UnitType::new(3))
    );
    assert_eq!(cx.type_named(hero_scope, "tower"), None);
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

#[test]
#[should_panic(expected = "is asked for after the load took it")]
fn a_fact_asked_for_after_the_load_took_it_is_not_built_again() {
    with_cx(|mut cx| {
        cx.declare_fact::<Base>(|_| Ok(Base(3)));
        assert_eq!(cx.take_fact::<Base>().unwrap(), Some(Base(3)));
        let _ = cx.fact::<Base>();
    });
}
