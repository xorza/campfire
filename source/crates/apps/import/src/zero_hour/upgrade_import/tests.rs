use super::*;

#[test]
fn a_module_converts_by_its_triggers_effect_and_life_change_or_drops_by_the_first_it_fails() {
    let mut rules = ZeroHourRules::default();
    rules
        .upgrades
        .read(b"Upgrade Upgrade_A\nEnd\nUpgrade Upgrade_B\nEnd\n")
        .unwrap();
    let bonuses = BTreeMap::new();
    let sources = ModuleSources {
        rules: &rules,
        bonuses: &bonuses,
    };
    let module = |effect, triggers: &[&str]| IniUpgradeModule {
        effect,
        triggers: triggers.iter().map(|&name| name.to_owned()).collect(),
        conflicts: vec!["Upgrade_B".to_owned(), "Upgrade_None".to_owned()],
        all: false,
    };
    let life = |add, change| UpgradeEffect::MaxHealth { add, change };
    let mut import = UpgradeImport::default();
    let mut convert = |module| import.module(sources, &module, &[]);
    // 50 more life by A, unless B; a conflict no file defines goes.
    let entry = convert(module(
        life(50.0, HealthChange::AddCurrentToo),
        &["Upgrade_A"],
    ));
    let declared = |name: &str| DeclaredName::new(name).unwrap();
    assert_eq!(
        entry,
        Ok(UpgradeData {
            triggers: vec![declared("upgrade_a")],
            all: false,
            conflicts: vec![declared("upgrade_b")],
            when: None,
            modifier: declared("upgrade_a_life_50"),
        })
    );
    // No trigger, or one no file defines; a change that is not the engine's; a life of a
    // fraction, of none, or below none.
    for (effect, triggers, cause) in [
        (UpgradeEffect::Armor, &[][..], UpgradeDropCause::NoUpgrade),
        (
            UpgradeEffect::Armor,
            &["Upgrade_A", "Upgrade_C"][..],
            UpgradeDropCause::NoUpgrade,
        ),
        (
            life(50.0, HealthChange::FullyHeal),
            &["Upgrade_A"][..],
            UpgradeDropCause::HealthChange,
        ),
        (
            life(12.5, HealthChange::AddCurrentToo),
            &["Upgrade_A"][..],
            UpgradeDropCause::BadNumber,
        ),
        (
            life(0.0, HealthChange::AddCurrentToo),
            &["Upgrade_A"][..],
            UpgradeDropCause::BadNumber,
        ),
        (
            life(-25.0, HealthChange::AddCurrentToo),
            &["Upgrade_A"][..],
            UpgradeDropCause::BadNumber,
        ),
        (
            UpgradeEffect::Other("StealthUpgrade".to_owned()),
            &["Upgrade_A"][..],
            UpgradeDropCause::Effect,
        ),
    ] {
        assert_eq!(
            convert(module(effect, triggers)),
            Err(cause),
            "{triggers:?}"
        );
    }
    // A second life of A's on a type that holds the first adds on its own, of a name of its own;
    // a third takes the next.
    let mut held = vec![entry.unwrap()];
    let again = module(life(50.0, HealthChange::AddCurrentToo), &["Upgrade_A"]);
    for name in ["upgrade_a_life_50_2", "upgrade_a_life_50_3"] {
        let next = import.module(sources, &again, &held).unwrap();
        assert_eq!(next.modifier, declared(name));
        held.push(next);
    }
    // An armor of A's twice is one flag, of one modifier.
    let armor = module(UpgradeEffect::Armor, &["Upgrade_A"]);
    let held = [import.module(sources, &armor, &[]).unwrap()];
    let twice = import.module(sources, &armor, &held).unwrap();
    assert_eq!(twice.modifier, held[0].modifier);
    // What converted keeps its modifiers: A's and B's, and the effect's, of the life stat.
    assert_eq!(
        import.upgrades,
        BTreeSet::from([declared("upgrade_a"), declared("upgrade_b")])
    );
    let effect = &import.effects[&declared("upgrade_a_life_50")];
    assert_eq!(
        effect.stats,
        BTreeMap::from([(rules.names.life_stat.clone(), Number::from(50))])
    );
}

#[test]
fn two_upgrades_of_one_modifier_s_name_are_refused() {
    let mut rules = ZeroHourRules::default();
    rules
        .upgrades
        .read(b"Upgrade Upgrade_A\nEnd\nUpgrade UPGRADE_A\nEnd\n")
        .unwrap();
    let production = ProductionImport::default();
    let combat = CombatImport::default();
    let refused = UpgradeImport::of(&rules, std::iter::empty(), &production, &combat);
    assert!(
        matches!(&refused, Err(ZeroHourError::UpgradeName(name)) if name == "Upgrade_A"),
        "{refused:?}"
    );
}
