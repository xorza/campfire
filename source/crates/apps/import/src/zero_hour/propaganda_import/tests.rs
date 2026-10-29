use super::*;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::import_names::ImportNames;

/// A speaker tower of 2 % a second, 4 % upgraded, and a carrier whose upgrade makes a portable
/// tower of 1 %, 2 % upgraded, inside it, each upgraded by `Upgrade_Slogans`.
const TOWERS: &str = "Object Speaker
  KindOf = STRUCTURE
  Behavior = PropagandaTowerBehavior ModuleTag_01
    Radius = 150.0
    HealPercentEachSecond = 2%
    UpgradeRequired = Upgrade_Slogans
    UpgradedHealPercentEachSecond = 4%
  End
End
Object Carrier
  Behavior = ObjectCreationUpgrade ModuleTag_01
    UpgradeObject = OCL_Tower
    TriggeredBy = Upgrade_Tower
    ConflictsWith = Upgrade_Gun
  End
End
Object Tower
  KindOf = PORTABLE_STRUCTURE
  Behavior = PropagandaTowerBehavior ModuleTag_01
    Radius = 150.0
    HealPercentEachSecond = 1%
    UpgradeRequired = Upgrade_Slogans
    UpgradedHealPercentEachSecond = 2%
  End
End
";

#[test]
fn a_tower_is_an_aura_of_its_heal_an_upgrade_of_its_own_type_while_built_or_of_its_carrier() {
    let mut rules = ZeroHourRules {
        objects: ObjectIni::of(&[TOWERS]),
        ..ZeroHourRules::default()
    };
    let list = "ObjectCreationList OCL_Tower\n  CreateObject\n    ObjectNames = Tower\n    \
                ContainInsideSourceObject = Yes\n  End\nEnd\n";
    rules.creation_lists.read(list.as_bytes()).unwrap();
    rules.game_data.enthusiastic_rate = 1.25;
    rules.game_data.subliminal_rate = 1.5;
    let mut names = ImportNames::default();
    for object in ["Speaker", "Carrier"] {
        let object = rules.objects.object(object.as_bytes()).unwrap();
        names.unit_type(object, &rules).unwrap();
    }
    let names = names.into_production().into_projectiles();
    let import = PropagandaImport::of(&rules, names.types()).unwrap();
    let declared = |name: &str| DeclaredName::new(name).unwrap();
    // The speaker's aura is an upgrade of no trigger, its upgraded aura one of `Upgrade_Slogans`,
    // both while it is no site; the carrier's, upgrades of its module's trigger, and of that and
    // `Upgrade_Slogans`, each of the module's conflicts.
    let entry = |triggers: &[&str], all, conflicts: &[&str], modifier| UpgradeData {
        triggers: triggers.iter().map(|name| declared(name)).collect(),
        all,
        conflicts: conflicts.iter().map(|name| declared(name)).collect(),
        when: None,
        modifier: declared(modifier),
    };
    let built = |triggers, modifier| UpgradeData {
        when: Some(rules.names.built.clone()),
        ..entry(triggers, true, &[], modifier)
    };
    assert_eq!(
        import.upgrades,
        BTreeMap::from([
            (
                TypeName::named("carrier"),
                vec![
                    entry(
                        &["upgrade_tower"],
                        false,
                        &["upgrade_gun"],
                        "tower_propaganda"
                    ),
                    entry(
                        &["upgrade_tower", "upgrade_slogans"],
                        true,
                        &["upgrade_gun"],
                        "tower_propaganda_upgraded"
                    ),
                ]
            ),
            (
                TypeName::named("speaker"),
                vec![
                    built(&[], "speaker_propaganda"),
                    built(&["upgrade_slogans"], "speaker_propaganda_upgraded"),
                ]
            ),
        ])
    );
    // Each aura holds its heal on the allied units but structures within 150 m; a heal adds
    // its share of life a second and its rate of fire less 1, from one tower at a time; the
    // upgraded adds what its share passes the base's by: the speaker's 4 % − 2 %, in the
    // game's `f32`.
    let heal = |share: f32, rate: f32| PropagandaModifier {
        stats: BTreeMap::from([
            (
                rules.names.life_regen_share.clone(),
                Number::from(game_float::to_num(share).unwrap()),
            ),
            (
                rules.names.propaganda_rate.clone(),
                Number::from(game_float::to_num(rate).unwrap()),
            ),
        ]),
        held_sources: Some(HeldSources::One),
        aura: None,
    };
    let aura = |modifier: &str| PropagandaModifier {
        stats: BTreeMap::new(),
        held_sources: None,
        aura: Some(PropagandaAura {
            radius: Number::from(game_float::to_num(150.0).unwrap()),
            affects: FilterData::parse("allies:!structure").unwrap(),
            modifier: declared(modifier),
        }),
    };
    let modifiers = &import.modifiers;
    assert_eq!(
        modifiers[&declared("speaker_propaganda")],
        aura("speaker_propaganda_heal")
    );
    assert_eq!(
        modifiers[&declared("speaker_propaganda_heal")],
        heal(0.02, 0.25)
    );
    assert_eq!(
        modifiers[&declared("speaker_propaganda_upgraded_heal")],
        heal(0.04 - 0.02, 0.5)
    );
    assert_eq!(
        modifiers[&declared("tower_propaganda_heal")],
        heal(0.01, 0.25)
    );
    assert_eq!(modifiers.len(), 8);
    assert_eq!(
        import.named,
        ["upgrade_gun", "upgrade_slogans", "upgrade_tower"]
            .map(declared)
            .into()
    );
}
