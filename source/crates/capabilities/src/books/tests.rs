use std::collections::BTreeMap;
use std::fmt::Debug;
use std::panic::{self, AssertUnwindSafe};

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::resource::Resource;
use campfire_math::Num;
use campfire_sim::{Breach, Capability, HostRule, SimComponent, SimResource};

use super::*;
use crate::combat::combat_book::CombatBook;
use crate::declare::cache::Cache;
use crate::declare::callbacks::Callbacks;
use crate::declare::declarations::{ColumnFill, Declarations};
use crate::declare::kind_spawn::KindSpawn;
use crate::declare::kind_visitor::KindVisitor;
use crate::declare::module_set::ModuleSet;
use crate::declare::report::Report;
use crate::declare::scratch::Scratch;
use crate::orders::ai::Ai;
use crate::release::r#match::Match;
use crate::scripts::api_builder::ApiBuilder;
use crate::scripts::call_part::CallPart;
use crate::scripts::frame::Frame;
use crate::state_types::StateTypes;
use crate::state_types::replication::Replication;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::param_book::ParamBook;
use crate::stats::stat_book::StatBook;
use crate::stats::stat_rule::StatRule;
use crate::units::engine_tag::EngineTag;
use crate::units::entity_kind::EntityKind;
use crate::units::row_reads::RowReads;
use crate::units::view_column::ViewColumn;
use crate::values::stat::Stat;

/// A book no module declares.
#[derive(Resource, Debug, Default)]
struct StrayBook;

impl Book for StrayBook {
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View, _: Option<&mut Frame>) {}
}

/// Builds every book a module declares, empty, as a builder that built them all would.
#[derive(Debug, Default)]
struct EveryBook(BuiltBooks);

impl StateTypes for EveryBook {
    fn component<C: Replication>(&mut self) {}

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {}

    fn resource<R: SimResource + Default>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {}
}

impl KindVisitor for EveryBook {
    fn kind<K: EntityKind>(&mut self) {}
}

impl Declarations for EveryBook {
    fn module(&mut self, _: Option<Capability>) {}

    fn require<H: SimComponent, C: SimComponent>(&mut self) {}

    fn host<C: SimComponent, H: HostRule>(&mut self) {}

    fn check<C: SimComponent>(&mut self, _: fn(&World) -> Result<(), Breach>) {}

    fn book<B: Book>(&mut self) {
        self.0.add(B::default());
    }

    fn sets<S: ModuleSet>(&mut self) {}

    fn scratch<R: Scratch>(&mut self) {}

    fn cache<R: Cache>(&mut self) {}

    fn report<R: Report>(&mut self) {}

    fn calls<C: Callbacks>(&mut self) {}

    fn callback<C: Callbacks>(&mut self, _: C::Key, _: C::Callback) {}

    fn column<C: ViewColumn + Default, D: RowReads>(&mut self, _: ColumnFill<D, C>) {}

    fn call_part<P: CallPart + Default>(&mut self) {}

    fn part<H: HostRule + SimComponent, C: SimComponent>(
        &mut self,
        _: fn(&KindSpawn<'_, H>) -> Option<C>,
    ) {
    }

    fn engine_tags(&mut self, _: &'static [EngineTag]) {}

    fn api(&mut self, _: fn(&mut ApiBuilder<'_>)) {}
}

/// Every book a module declares, built empty.
fn every_book() -> BuiltBooks {
    let mut every = EveryBook::default();
    Release::declare(Release::capabilities(), &mut every);
    every.0
}

/// The message of the panic `run` raises.
fn panic_of(run: impl FnOnce()) -> String {
    let caught = panic::catch_unwind(AssertUnwindSafe(run)).unwrap_err();
    caught.downcast_ref::<String>().cloned().unwrap()
}

#[test]
fn a_book_goes_in_only_for_its_owner_and_the_scripts_it_needs() {
    let stats = Release::capability_set(&[Capability::Stats]).unwrap();
    let none = Release::capability_set(&[]).unwrap();
    let orders = Release::capability_set(&[
        Capability::Stats,
        Capability::Combat,
        Capability::Navigation,
        Capability::Orders,
    ])
    .unwrap();
    // A capability's book is held with its owner; an AI's with orders and scripts both; a core
    // book in every match.
    assert!(ModifierBook::held(stats, false) && !ModifierBook::held(none, true));
    assert!(ByType::<Ai>::held(orders, true) && !ByType::<Ai>::held(orders, false));
    assert!(!ByType::<Ai>::held(stats, true));
    assert!(ParamBook::held(none, false));
    // The install puts each book the match holds and drops the rest: a match of stats holds the
    // stat book, which no longer is the empty one its declaration put, and no combat book.
    let mut game = Match::client(&[Capability::Stats]);
    let mut built = every_book();
    let rules = BTreeMap::from([(Stat::named("armor").unwrap(), StatRule::default())]);
    built.take::<StatBook>().unwrap();
    built.add(StatBook::new(&rules, Num::ONE));
    Books::fill(&mut built, &mut game, stats, false);
    assert!(built.is_empty());
    assert_eq!(game.resource::<StatBook>().len(), 1);
    assert!(!game.contains_resource::<CombatBook>());
    // A book its installed owner did not declare, a book declared and not built, and one built
    // and not declared are each a logic error.
    let said = panic_of(|| Books::load(&mut game, CombatBook::default()));
    assert!(said.ends_with("is declared by its owner"), "{said}");
    let mut missing = every_book();
    missing.take::<ModifierBook>().unwrap();
    let said = panic_of(|| Books::fill(&mut missing, &mut game, stats, false));
    assert!(
        said.ends_with("ModifierBook is declared and built"),
        "{said}"
    );
    let mut stray = every_book();
    stray.add(StrayBook);
    let said = panic_of(|| Books::fill(&mut stray, &mut game, stats, false));
    assert!(said.starts_with("each built book is declared"), "{said}");
    assert!(said.contains("StrayBook"), "{said}");
}
