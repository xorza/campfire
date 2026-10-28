use campfire_codec::Toml;

use super::*;
use crate::zero_hour::format::ini::object_ini::ini_experience::IniStart;

fn thresholds(levels: [i64; 3]) -> Thresholds {
    Thresholds::new(levels.map(Num::int)).unwrap()
}

#[test]
fn a_type_s_thresholds_start_at_0_and_rise_and_the_track_takes_the_commonest() {
    // A Crusader needs 0, 200, 300 and 600: levels 2 to 4 at 200, 300 and 600. A list of no 0
    // first, or one that does not rise, gains no level.
    let required = |required| IniExperience {
        required,
        trainable: true,
        ..IniExperience::default()
    };
    assert_eq!(
        VeterancyImport::thresholds(&required([0, 200, 300, 600])),
        Some(thresholds([200, 300, 600]))
    );
    for refused in [[10, 200, 300, 600], [0, 200, 200, 600], [0, 0, 1, 2]] {
        assert_eq!(
            VeterancyImport::thresholds(&required(refused)),
            None,
            "{refused:?}"
        );
    }
    // Two of 100, 200 and 400 beside one of 200, 300 and 600; at a tie, the least.
    let [common, crusader] = [thresholds([100, 200, 400]), thresholds([200, 300, 600])];
    let three = [common.clone(), crusader.clone(), common.clone()];
    assert_eq!(
        VeterancyImport::commonest(three.iter()),
        Some(common.clone())
    );
    let tie = [crusader, common.clone()];
    assert_eq!(VeterancyImport::commonest(tie.iter()), Some(common));
    assert_eq!(VeterancyImport::commonest([].iter()), None);
}

#[test]
fn a_level_s_modifier_adds_its_bonuses_less_1_and_raises_life_by_its_share() {
    // A veteran of 110 % damage, 120 % rate of fire and 120 % health: 0.1 more damage, 0.2 more
    // rate of fire, and 20 % more life, each the game's `f32` less 1; a bonus of 1 adds nothing.
    let bonus = VeterancyBonus {
        damage: 1.1,
        rate_of_fire: 1.2,
        health: 1.0,
    };
    let names = RulesNames::default();
    let modifier = LevelModifier::of(bonus, &names).unwrap();
    let text = Toml::write(&modifier);
    let damage = game_float::to_num(1.1_f32 - 1.0).unwrap();
    let rate = game_float::to_num(1.2_f32 - 1.0).unwrap();
    assert_eq!(
        text,
        format!("[stats]\nveterancy_damage = \"{damage}\"\nveterancy_rate = \"{rate}\"\n")
    );
    let raised = LevelModifier::of(
        VeterancyBonus {
            health: 1.5,
            ..bonus
        },
        &names,
    )
    .unwrap();
    assert_eq!(
        Toml::write(&raised),
        format!(
            "[stats]\nveterancy_damage = \"{damage}\"\nveterancy_rate = \"{rate}\"\n\n[stats.life]\npct = \"0.5\"\n"
        )
    );
    let infinite = VeterancyBonus {
        damage: f32::INFINITY,
        ..bonus
    };
    assert!(LevelModifier::of(infinite, &names).is_err());
}

#[test]
fn a_veterancy_gain_starts_its_unit_a_level_past_the_game_s_by_its_science_s_perk() {
    // The Tank General's Battlemaster: veteran always, elite by `SCIENCE_BattlemasterTraining`;
    // a gain at regular starts nothing.
    let start = |level, science: Option<&str>| IniStart {
        level,
        science: science.map(str::to_owned),
    };
    let battlemaster = IniExperience {
        starts: vec![
            start(1, None),
            start(2, Some("SCIENCE_BattlemasterTraining")),
            start(0, None),
        ],
        ..IniExperience::default()
    };
    let level = |level| Level::new(level).unwrap();
    assert_eq!(
        VeterancyImport::starts(&battlemaster).unwrap(),
        [
            StartData {
                level: level(2),
                requires: None,
            },
            StartData {
                level: level(3),
                requires: DeclaredName::new("science_battlemastertraining"),
            },
        ]
    );
}
