use super::*;

#[test]
fn each_flag_tests_its_part_of_a_unit_and_any_other_none() {
    let mut game_data = GameDataIni::default();
    game_data.damaged_threshold = 0.5;
    game_data.really_damaged_threshold = 0.25;
    // A power plant's rods and a model condition of `USER_1` an upgrade sets.
    let plant = "Object Plant\n  Behavior = PowerPlantUpgrade M1\n    TriggeredBy = Upgrade_A\n  End\n  \
                 Behavior = ModelConditionUpgrade M2\n    TriggeredBy = Upgrade_A\n    \
                 ConditionFlag = USER_1\n  End\nEnd\n";
    let objects = ObjectIni::of(&[plant]);
    let flags = LookFlags::of(&game_data, &RulesNames::default(), &objects).unwrap();
    let test = |flag: &str| flags.condition(flag).unwrap().test;
    let [quarter, half] = [4, 2].map(|part| Num::from_bits(Num::ONE.to_bits() / part));
    assert_eq!(test("night"), ConditionTest::Map);
    assert_eq!(test("snow"), ConditionTest::Map);
    // Damaged above a quarter and at most half; really damaged above none and at most a
    // quarter.
    assert_eq!(
        test("damaged"),
        ConditionTest::Life(LifeShare::new(quarter, half).unwrap())
    );
    assert_eq!(
        test("reallydamaged"),
        ConditionTest::Life(LifeShare::new(Num::ZERO, quarter).unwrap())
    );
    assert_eq!(test("moving"), ConditionTest::Moving { still_ms: 250 });
    assert_eq!(test("attacking"), ConditionTest::Attacking);
    // Dying, and rubble, the damage state of no life, which only a dead unit holds.
    assert_eq!(test("dying"), ConditionTest::Dead);
    assert_eq!(test("rubble"), ConditionTest::Dead);
    assert_eq!(
        test("weaponset_player_upgrade"),
        ConditionTest::Tag {
            tag: RulesNames::default().weapon_upgrade
        }
    );
    // A site before and after a builder first built it, and one that grew in the tick.
    assert_eq!(
        [test("awaiting_construction"), test("partially_constructed")],
        [SiteStage::Waiting, SiteStage::Started].map(|stage| ConditionTest::Site { stage })
    );
    assert_eq!(
        test("actively_being_constructed"),
        ConditionTest::Tag {
            tag: DeclaredName::new("being_built").unwrap()
        }
    );
    // Each phase of an attack of slot A, B or C, of the look's group at its place, and the use of
    // a slot at any phase; a slot past C is none.
    let attack = |group, phase| ConditionTest::Attack { group, phase };
    assert_eq!(
        [
            test("preattack_a"),
            test("firing_b"),
            test("between_firing_shots_c"),
            test("reloading_a"),
            test("using_weapon_b"),
            test("firing_d"),
        ],
        [
            attack(0, Some(AttackPhase::PreAttack)),
            attack(1, Some(AttackPhase::Firing)),
            attack(2, Some(AttackPhase::Between)),
            attack(0, Some(AttackPhase::Reloading)),
            attack(1, None),
            ConditionTest::Never,
        ]
    );
    // The flags an upgrade sets test the tags of their names, which the upgrades' modifiers give;
    // a flag none sets, none.
    let tag = |name: &str| ConditionTest::Tag {
        tag: DeclaredName::new(name).unwrap(),
    };
    assert_eq!(test("power_plant_upgraded"), tag("power_plant_upgraded"));
    assert_eq!(test("user_1"), tag("user_1"));
    assert_eq!(test("user_2"), ConditionTest::Never);
    assert_eq!(test("garrisoned"), ConditionTest::Never);
    assert_eq!(
        flags.condition("door_1_opening").unwrap().name.as_str(),
        "door_1_opening"
    );
    // A flag that is no name; thresholds that do not fall, or reach none.
    assert!(
        matches!(flags.condition("1st"), Err(ZeroHourError::ConditionFlag(flag)) if flag == "1st")
    );
    for [damaged, really] in [[0.25, 0.5], [0.5, 0.0], [1.5, 0.5]] {
        game_data.damaged_threshold = damaged;
        game_data.really_damaged_threshold = really;
        assert!(
            matches!(
                LookFlags::of(&game_data, &RulesNames::default(), &objects),
                Err(ZeroHourError::DamageThresholds)
            ),
            "{damaged} {really}"
        );
    }
}
