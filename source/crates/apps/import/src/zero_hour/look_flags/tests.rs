use super::*;

#[test]
fn each_flag_tests_its_part_of_a_unit_and_any_other_none() {
    let mut game_data = GameDataIni::default();
    game_data.damaged_threshold = 0.5;
    game_data.really_damaged_threshold = 0.25;
    let flags = LookFlags::of(&game_data, &RulesNames::default()).unwrap();
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
    assert_eq!(
        test("weaponset_player_upgrade"),
        ConditionTest::Tag {
            tag: RulesNames::default().weapon_upgrade
        }
    );
    assert_eq!(test("rubble"), ConditionTest::Never);
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
                LookFlags::of(&game_data, &RulesNames::default()),
                Err(ZeroHourError::DamageThresholds)
            ),
            "{damaged} {really}"
        );
    }
}
