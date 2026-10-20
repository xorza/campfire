use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

use bevy_ecs::resource::Resource;
use campfire_combat::CombatBook;
use campfire_common::SegmentSeed;
use campfire_declarations::internals::BookLoad;
use campfire_declarations::{Book, ByType, Role};
use campfire_math::Num;
use campfire_orders::Ai;
use campfire_script::{MatchNames, ScriptLimits, ScriptPools};
use campfire_sim::{Capability, SimWorld};
use campfire_stats::{ModifierBook, ParamBook, StatBook, StatRule};
use campfire_values::Stat;

use crate::books::Books;
use crate::release::Release;
use crate::release::r#match::Match;
use crate::release::match_kind::MatchKind;
use crate::release::match_setup::MatchSetup;

/// A book no module declares.
#[derive(Resource, Debug, Default)]
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

/// A match of the rules of stats alone, of `books`.
fn stats_match(books: Books) -> Match {
    let set = Release::capability_set(&[Capability::Stats]).unwrap();
    let setup = MatchSetup::new(SegmentSeed::ZERO, Match::TEST_RATE, Role::Rules).set(set);
    Match::new(setup, books, MatchKind::Rules)
}

#[test]
fn a_book_goes_in_only_for_its_owner_and_the_scripts_it_needs() {
    // A book only scripts read, as an AI's, is held where scripts run; any other in every role,
    // of a module the match installs.
    let scripts = Role::Server(ScriptPools {
        limits: ScriptLimits::ROOMY,
        players: 1,
    });
    let (unscripted, client) = (Role::Rules, Role::Client);
    assert!(ByType::<Ai>::is_held(&scripts) && !ByType::<Ai>::is_held(&unscripted));
    assert!(!ByType::<Ai>::is_held(&client));
    assert!(ModifierBook::is_held(&client) && ParamBook::is_held(&client));
    // The build puts each book the match holds, its builder's, before its module installs, and
    // drops the rest: a match of stats holds the stat book built, and no combat book.
    let mut books = Release::empty_books(Match::TEST_RATE);
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
    let mut open = Match::unscripted(&[Capability::Stats]);
    StatBook::new(&rules, Num::ONE).load_into(&mut open);
    open.step();
    assert_eq!(open.resource::<StatBook>().len(), 1);
    let mut missing = Release::empty_books(Match::TEST_RATE);
    missing.built.take::<ModifierBook>().unwrap();
    let said = panic_of(|| drop(stats_match(missing)));
    assert!(
        said.ends_with("ModifierBook is declared and built"),
        "{said}"
    );
    let mut stray = Release::empty_books(Match::TEST_RATE);
    stray.built.add(StrayBook);
    let said = panic_of(|| drop(stats_match(stray)));
    assert!(said.starts_with("each built book is declared"), "{said}");
    assert!(said.contains("StrayBook"), "{said}");
}
