use bevy_ecs::schedule::Schedules;
use campfire_capabilities::{Books, Match, MatchKind, MatchSetup, Release};
use campfire_common::SegmentSeed;
use campfire_declarations::Role;
use campfire_sim::SimUpdate;
use campfire_test_packages::TestMode;

/// The names of the systems a server's match of the rules alone, of every capability, of `books`
/// installs, in order.
fn installed(books: Books) -> Vec<String> {
    let setup = MatchSetup::new(SegmentSeed::ZERO, Match::TEST_RATE, Role::Rules);
    let world = Match::new(setup, books, MatchKind::Rules);
    let schedules = world.resource::<Schedules>();
    let systems = schedules.get(SimUpdate).unwrap().systems().unwrap();
    let mut names: Vec<String> = systems
        .map(|(_, system)| system.name().to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn a_feature_installs_its_passes_only_in_a_match_whose_books_have_it() {
    // A mode of no data installs no pass of a feature; one of each fact installs each pass of
    // its feature, and every other system alike: the channels' and the toggles', the slow
    // weapons' hits, the toggles that cost an attack, the turrets' aim, the supply's count, the
    // modifiers' intervals, the regen and the sight.
    let none = installed(Release::empty_books(
        Match::TEST_RATE,
        Release::capabilities(),
    ));
    let every = installed(TestMode::Features.packages().books(Match::TEST_RATE));
    let mut added = every;
    for name in &none {
        let at = added.iter().position(|held| held == name);
        added.remove(at.expect("a mode of every fact installs every system of none"));
    }
    let passes = [
        "campfire_abilities::channels::Channeling::predict_channels",
        "campfire_abilities::toggles::Toggles::run_toggles",
        "campfire_combat::attacks::Attacks::land_held",
        "campfire_combat::attacks::Attacks::pay_attack_toggles",
        "campfire_combat::turret::aiming::Aiming::aim",
        "campfire_production::player_supply::PlayerSupply::refresh",
        "campfire_stats::modifier_intervals::ModifierIntervals::count_intervals",
        "campfire_stats::refresh::Refresh::regenerate",
        "campfire_vision::seeing::Seeing::refill",
        "campfire_vision::seeing::Seeing::see",
    ];
    assert_eq!(added, passes);
}
