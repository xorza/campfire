use super::*;
use crate::books::test_package::TestPackage;

/// A mode package with each fact that decides an install: a map's vision grid, a count of
/// supply, a pool that regenerates, a modifier of an interval, and a type of a turret, a slow
/// weapon, toggles that cost an attack and a second, and a channel.
pub(super) const EVERY_SYSTEM: TestPackage<'_> = TestPackage {
    mode: r#"
name = "mode"
supply = { max = 10 }
combat = { damage_kinds = ["physical"], life = "health" }
slots = [{ name = "basic" }]
pools = { health = { max = "health", regen = "health_regen" } }

[stats]
health = {}
health_regen = {}
attack_speed = {}
attack_damage = {}
"#,
    map: r#"
bounds = { min = [-4, -4], max = [4, 4] }
grid = { cell = "1.0" }
"#,
    content: r#"
[units.tank]
pools = ["health"]
slots = { basic = ["shell", "guard", "burn", "focus"] }
turret = { turn_rate = "90", tolerance_deg = "5", slot = "basic" }
stats = { health = { base = 10 } }
combat = {}

[actions.shell]
name = "action-shell"
kind = "attack"
targeting = "enemies"
range = "6.0"
rate = "attack_speed"
damage = "attack_damage"
damage_kind = "physical"
speed = "20"

[actions.guard]
name = "action-guard"
targeting = "none"
toggle = { cost_per_attack = { health = 1 } }

[actions.burn]
name = "action-burn"
targeting = "none"
toggle = { cost_per_second = { health = 1 } }

[actions.focus]
name = "action-focus"
targeting = "none"
channel = { duration_ms = 1000, tick_ms = 500 }

[modifiers.pulse]
interval_ms = 1000
"#,
};

/// The names of the systems a server's match of the rules alone, of every capability, of `books`
/// installs, in order.
fn installed(books: Books) -> Vec<String> {
    let setup = MatchSetup::new(SegmentSeed::ZERO, Match::TEST_RATE, Role::Rules);
    let world = Match::new(setup.set(Release::capabilities()), books, MatchKind::Rules);
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
    let none = installed(Release::empty_books(Match::TEST_RATE));
    let every = installed(EVERY_SYSTEM.books(Match::TEST_RATE, &[]));
    let mut added = every;
    for name in &none {
        let at = added.iter().position(|held| held == name);
        added.remove(at.expect("a mode of every fact installs every system of none"));
    }
    let passes = [
        "campfire_abilities::channels::Channels::predict_channels",
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
