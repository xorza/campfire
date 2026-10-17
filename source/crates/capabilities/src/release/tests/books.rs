use std::collections::BTreeMap;
use std::panic::{self, AssertUnwindSafe};

use bevy_ecs::resource::Resource;
use campfire_combat::CombatBook;
use campfire_common::SegmentSeed;
use campfire_declare::{Book, Role};
use campfire_math::Num;
use campfire_orders::Ai;
use campfire_script::{ScriptLimits, ScriptPools, View};
use campfire_sim::Capability;
use campfire_stats::{ModifierBook, ParamBook, StatBook, StatRule};
use campfire_values::Stat;

use campfire_declare::ByType;

use crate::books::Books;
use crate::release::Release;
use crate::release::r#match::Match;
use crate::release::match_kind::MatchKind;
use crate::release::match_spec::MatchSpec;

/// A book no module declares.
#[derive(Resource, Debug, Default)]
struct StrayBook;

impl Book for StrayBook {
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}

/// The message of the panic `run` raises.
fn panic_of(run: impl FnOnce()) -> String {
    let caught = panic::catch_unwind(AssertUnwindSafe(run)).unwrap_err();
    caught.downcast_ref::<String>().cloned().unwrap()
}

/// A match of the rules of stats alone, of `books`.
fn stats_match(books: Books) -> Match {
    let set = Release::capability_set(&[Capability::Stats]).unwrap();
    let spec = MatchSpec::new(SegmentSeed::ZERO, Match::TEST_RATE, Role::Rules).set(set);
    Match::new(spec, books, MatchKind::Rules)
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
    assert!(ByType::<Ai>::held(&scripts) && !ByType::<Ai>::held(&unscripted));
    assert!(!ByType::<Ai>::held(&client));
    assert!(ModifierBook::held(&client) && ParamBook::held(&client));
    // The build puts each book the match holds, its builder's, before its owner installs, and
    // drops the rest: a match of stats holds the stat book built, and no combat book.
    let mut books = Release::empty_books(Match::TEST_RATE);
    let rules = BTreeMap::from([(Stat::named("armor").unwrap(), StatRule::default())]);
    books.built.take::<StatBook>().unwrap();
    books.built.add(StatBook::new(&rules, Num::ONE));
    let mut game = stats_match(books);
    assert_eq!(game.resource::<StatBook>().len(), 1);
    assert!(!game.contains_resource::<CombatBook>());
    // A book its installed owner did not declare, a book declared and not built, and one built
    // and not declared are each a logic error.
    let said = panic_of(|| CombatBook::default().load_into(&mut game));
    assert!(said.ends_with("is declared by its owner"), "{said}");
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
