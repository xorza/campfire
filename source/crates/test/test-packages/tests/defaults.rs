//! A mode's defaults of its unit types' sections, which lie under each type's own.

use campfire_combat::{CombatData, OnDeath, TurretData};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_package::{DependentKind, LoadError, LoadProblem, ModePackages};
use campfire_package_format::ContentError;
use campfire_test_packages::{Edit, PackageTree, TestMode};

/// The death of each of `types` of the mode, then of each avatar it depends on, in order.
fn deaths(packages: &ModePackages, types: &[&str]) -> Vec<OnDeath> {
    let units = &packages.content().units;
    let of_type = types.iter().map(|name| {
        let unit = &units[&DeclaredName::new(name).unwrap()];
        unit.section::<CombatData>().unwrap().on_death
    });
    let avatars = packages.dependencies().iter().filter_map(|dependent| {
        let DependentKind::Avatar(avatar) = &dependent.kind else {
            return None;
        };
        Some(avatar.unit.section::<CombatData>().unwrap().on_death)
    });
    of_type.chain(avatars).collect()
}

fn load(mode: TestMode, edits: &[(&str, Edit<'_>)]) -> Result<ModePackages, LoadError> {
    PackageTree::of(mode).edit_all(edits.iter().copied()).load()
}

#[test]
fn a_modes_default_lies_under_each_unit_types_section_and_the_types_own_field_stays() {
    const MODE: &str = "modes/lane/data/mode.toml";
    const UNITS: &str = "modes/lane/data/units.toml";
    // The lane's creep and tower and its two heroes give `combat` with no death, which
    // despawns.
    let lane = load(TestMode::Lane, &[]).unwrap();
    assert_eq!(deaths(&lane, &["creep", "tower"]), [OnDeath::Despawn; 4]);
    // The mode's default makes each stay, but the tower, which gives its own.
    let tower = "attack_speed = { base = \"0.83\" } }\ncombat = {}";
    let own = "attack_speed = { base = \"0.83\" } }\ncombat = { on_death = \"despawn\" }";
    let defaulted = load(
        TestMode::Lane,
        &[
            (
                MODE,
                Edit::Append("\n[defaults.combat]\non_death = \"stay\"\n"),
            ),
            (UNITS, Edit::Replace(tower, own)),
        ],
    )
    .unwrap();
    assert_eq!(
        deaths(&defaulted, &["creep", "tower"]),
        [
            OnDeath::Stay,
            OnDeath::Despawn,
            OnDeath::Stay,
            OnDeath::Stay
        ]
    );
    // A default of a field its section does not read fails the mode's data, as does one of no
    // section of a unit type's.
    for (defaults, why) in [
        (
            "\n[defaults.combat]\nreach = 2\n",
            "defaults.combat.reach is no field of its section",
        ),
        (
            "\n[defaults.ground]\ncell = 2\n",
            "defaults.ground is no section of a unit type",
        ),
    ] {
        let error = load(TestMode::Lane, &[(MODE, Edit::Append(defaults))]).unwrap_err();
        let LoadProblem::Content(ContentError::Data { path, error }) = &*error.problem else {
            panic!("{error:?}");
        };
        assert_eq!(path.as_str(), "data/mode.toml");
        assert!(error.message().contains(why), "{}", error.message());
    }
}

#[test]
fn a_rules_packages_defaults_lie_under_the_modes_unit_types_and_the_modes_own_over_them() {
    const RULES: &str = "zero-hour/data/rules.toml";
    const MODE: &str = "test/engine/modes/skirmish/data/mode.toml";
    const UNITS: &str = "test/engine/modes/skirmish/data/units.toml";
    // The skirmish's center gives no death of its own, and the dozer its `despawn`; Zero Hour's
    // rules make a unit stay as it dies, in place of their own `despawn`.
    let center = (
        UNITS,
        Edit::Replace("combat = { on_death = \"despawn\" }", "combat = {}"),
    );
    let rules = (RULES, Edit::Set("defaults.combat.on_death", "\"stay\""));
    let ruled = load(TestMode::Skirmish, &[center, rules]).unwrap();
    assert_eq!(
        deaths(&ruled, &["center", "dozer"]),
        [OnDeath::Stay, OnDeath::Despawn]
    );
    // The mode's own default of the section lies over the rules package's.
    let mode = (
        MODE,
        Edit::Append("\n[defaults.combat]\non_death = \"despawn\"\n"),
    );
    let over = load(TestMode::Skirmish, &[center, rules, mode]).unwrap();
    assert_eq!(
        deaths(&over, &["center", "dozer"]),
        [OnDeath::Despawn, OnDeath::Despawn]
    );
}

#[test]
fn a_zero_hour_type_takes_the_games_fixed_rules_from_its_rules_data() {
    // A type of a turn rate alone, as the import writes one, and of a `combat` of no death: its
    // turret takes the original's tolerance, 0.035 radians to the nearest bit, and the weapon slot
    // kind, and it despawns as it dies.
    const UNITS: &str = "test/engine/modes/skirmish/data/units.toml";
    let gunner = "\n[units.gunner]\npools = [\"life\"]\nstats = { life = { base = 100 } }\ncombat = {}\n\
                  turret = { turn_rate = 60 }\n";
    let skirmish = load(TestMode::Skirmish, &[(UNITS, Edit::Append(gunner))]).unwrap();
    let unit = &skirmish.content().units[&DeclaredName::new("gunner").unwrap()];
    let tolerance = Num::from_bits(33_644_228);
    let weapon = DeclaredName::new("weapon").unwrap();
    let turret = TurretData::new(Num::int(60), tolerance, weapon).unwrap();
    assert_eq!(unit.section::<TurretData>(), Some(&turret));
    assert_eq!(
        unit.section::<CombatData>().unwrap().on_death,
        OnDeath::Despawn
    );
}
