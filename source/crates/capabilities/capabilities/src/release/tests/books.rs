use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

use bevy_ecs::resource::Resource;
use campfire_combat::CombatBook;
use campfire_common::SegmentSeed;
use campfire_declarations::internals::BookLoad;
use campfire_declarations::{Book, ById, Role};
use campfire_math::Num;
use campfire_orders::Ai;
use campfire_script::{BudgetSetup, MatchNames, ScriptLimits};
use campfire_sim::{Capability, SimWorld};
use campfire_stats::{ModifierBook, ParamBook, StatBook, StatRule};
use campfire_values::{Stat, UnitType};

use crate::books::Books;
use crate::release::Release;
use crate::release::r#match::Match;
use crate::release::match_kind::MatchKind;
use crate::release::match_setup::MatchSetup;

/// A book no module declares.
#[derive(Resource, Debug, Default)]
#[component(immutable)]
struct StrayBook;

impl Book for StrayBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

/// The message of the panic `run` raises.
fn panic_of(run: impl FnOnce()) -> String {
    let caught = panic::catch_unwind(AssertUnwindSafe(run)).unwrap_err();
    caught.downcast_ref::<String>().cloned().unwrap()
}

/// The books of a mode of stats alone, of no data.
fn stats_books() -> Books {
    let set = Release::capability_set(&[Capability::Stats]).unwrap();
    Release::empty_books(Match::TEST_RATE, set)
}

/// A match of the rules of stats alone, of `books`.
fn stats_match(books: Books) -> Match {
    let setup = MatchSetup::new(SegmentSeed::ZERO, Match::TEST_RATE, Role::Rules);
    Match::new(setup, books, MatchKind::Rules)
}

#[test]
fn a_book_goes_in_only_for_its_owner_and_the_scripts_it_needs() {
    // A book only scripts read, as an AI's, is held where scripts run; any other in every role,
    // of a module the match installs.
    let scripts = Role::Server(BudgetSetup {
        limits: ScriptLimits::ROOMY,
        players: 1,
    });
    let (unscripted, client) = (Role::Rules, Role::Client);
    assert!(ById::<UnitType, Ai>::is_held(&scripts) && !ById::<UnitType, Ai>::is_held(&unscripted));
    assert!(!ById::<UnitType, Ai>::is_held(&client));
    assert!(ModifierBook::is_held(&client) && ParamBook::is_held(&client));
    // The build puts each book the match holds, its builder's, before its module installs: a
    // match of stats holds the stat book built, and no combat book.
    let mut books = stats_books();
    let rules = BTreeMap::from([(Stat::named("armor").unwrap(), StatRule::default())]);
    books.built.take::<StatBook>().unwrap();
    books.built.add(StatBook::new(&rules, Num::ONE));
    let mut game = stats_match(books);
    assert_eq!(game.resource::<StatBook>().len(), 1);
    assert!(!game.contains_resource::<CombatBook>());
    // A book its installed module did not declare, a book declared and not built, one built and
    // not declared, and one loaded after the install are each a logic error.
    let said = panic_of(|| CombatBook::default().load_into(&mut game));
    assert!(said.ends_with("is declared by its module"), "{said}");
    // A book its module declared loads only before the match's systems install, as their install
    // reads it: into a test's match whose first tick has not come, not into one built.
    let said = panic_of(|| StatBook::new(&rules, Num::ONE).load_into(&mut game));
    assert!(
        said.ends_with("StatBook loads before its match's systems install"),
        "{said}"
    );
    let mut open = Match::builder(&[Capability::Stats])
        .role(Role::Rules)
        .build();
    StatBook::new(&rules, Num::ONE).load_into(&mut open);
    open.step();
    assert_eq!(open.resource::<StatBook>().len(), 1);
    let mut missing = stats_books();
    missing.built.take::<ModifierBook>().unwrap();
    let said = panic_of(|| drop(stats_match(missing)));
    assert!(
        said.ends_with("ModifierBook is declared and built"),
        "{said}"
    );
    let mut stray = stats_books();
    stray.built.add(StrayBook);
    let said = panic_of(|| drop(stats_match(stray)));
    assert!(said.starts_with("each built book is declared"), "{said}");
    assert!(said.contains("StrayBook"), "{said}");
}
