use crate::zero_hour::format::ini::ai_data_ini::samples::AI_DATA;
use crate::zero_hour::format::ini::command_button_ini::samples::COMMAND_BUTTONS;
use crate::zero_hour::format::ini::command_set_ini::samples::COMMAND_SETS;
use crate::zero_hour::format::ini::player_template_ini::samples::PLAYER_TEMPLATES;
use crate::zero_hour::format::ini::rank_ini::RankIni;
use crate::zero_hour::format::ini::rank_ini::samples::RANKS;
use crate::zero_hour::format::ini::science_ini::samples::SCIENCES;

use super::*;

/// The fixture's rules of ranks and sciences.
fn rules() -> ZeroHourRules {
    let mut rules = ZeroHourRules::default();
    rules.ranks.read(RANKS.as_bytes()).unwrap();
    rules.sciences.read(SCIENCES.as_bytes()).unwrap();
    rules.templates.read(PLAYER_TEMPLATES.as_bytes()).unwrap();
    rules.sets.read(COMMAND_SETS.as_bytes()).unwrap();
    rules.buttons.read(COMMAND_BUTTONS.as_bytes()).unwrap();
    rules.ai.read(AI_DATA.as_bytes()).unwrap();
    rules
}

fn name(text: &str) -> DeclaredName {
    DeclaredName::new(text).unwrap()
}

#[test]
fn the_ranks_are_a_player_track_and_each_science_a_perk_of_its_rank_and_its_prerequisites() {
    // Ranks 2 to 5 at 800, 1,500, 2,500 and 5,000 skill points; points 1, 1, 1, 1 and 3.
    let import = ScienceImport::of(&rules(), [].into_iter()).unwrap();
    let (track, data) = import.track.clone().unwrap();
    assert_eq!(track, name("rank"));
    assert_eq!(data.holder, TrackHolder::Player);
    assert_eq!(
        data.levels.get(),
        [800, 1500, 2500, 5000].map(Num::int).as_slice()
    );
    assert_eq!(data.points, Some(vec![1, 1, 1, 1, 3]));
    let share = import.share.clone().unwrap();
    assert_eq!(
        share.to,
        ExperienceTo::Player {
            killer: rules().names.enemies
        }
    );
    // America's rank 3 set buys the paradrop's first two levels: the second of cost 1, at rank
    // 3, requires the first, which requires America's science, its rank's science a level; that
    // science of cost 0, which no player buys, America starts with.
    let perk = |cost, requires: &[&str], level: Option<u32>, modifier: &str| PerkData {
        cost: NonZeroU32::new(cost),
        requires: requires.iter().map(|&text| name(text)).collect(),
        track: level.map(|_| name("rank")),
        level: level.and_then(Level::new),
        modifier: name(modifier),
    };
    let expected = BTreeMap::from([
        (
            name("science_america"),
            perk(0, &[], None, "science_america"),
        ),
        (
            name("science_paradrop1"),
            perk(1, &["science_america"], Some(3), "science_paradrop1"),
        ),
        (
            name("science_paradrop2"),
            perk(1, &["science_paradrop1"], Some(3), "science_paradrop2"),
        ),
    ]);
    assert_eq!(import.perks, expected);
    assert_eq!(
        import.intrinsic,
        BTreeMap::from([(name("america"), vec![name("science_america")])])
    );
    // America's side's two skill sets, which its bot buys.
    let sets = [
        vec![name("science_paradrop1"), name("science_paradrop2")],
        vec![name("science_paradrop2")],
    ];
    assert_eq!(
        import.skill_sets,
        BTreeMap::from([(name("america"), sets.to_vec())])
    );
}

#[test]
fn ranks_rise_from_1_at_no_points_and_a_science_needs_a_rank_the_game_has() {
    let fails = |ranks: &str, sciences: &str| {
        let mut rules = rules();
        rules.ranks = RankIni::default();
        rules.ranks.read(ranks.as_bytes()).unwrap();
        rules.sciences.read(sciences.as_bytes()).unwrap();
        ScienceImport::of(&rules, [].into_iter()).err()
    };
    let two = "Rank 1\nEnd\nRank 2\n SkillPointsNeeded = 10\nEnd\n";
    // A first rank of points, a gap in the numbers, and a rank of no more than the one before.
    for ranks in [
        "Rank 1\n SkillPointsNeeded = 5\nEnd\nRank 2\n SkillPointsNeeded = 10\nEnd\n",
        "Rank 1\nEnd\nRank 3\n SkillPointsNeeded = 10\nEnd\n",
        "Rank 1\nEnd\nRank 2\nEnd\n",
    ] {
        assert!(
            matches!(fails(ranks, ""), Some(ZeroHourError::Ranks)),
            "{ranks}"
        );
    }
    // Of two ranks, the paradrop's rank 3 is past the last, its second level the first read;
    // a prerequisite no file defines.
    assert!(matches!(
        fails(two, ""),
        Some(ZeroHourError::ScienceRank(science)) if science == "SCIENCE_Paradrop2"
    ));
    let lost = "Science SCIENCE_Paradrop2\n PrerequisiteSciences = SCIENCE_Lost\nEnd\n";
    assert!(matches!(
        fails(RANKS, lost),
        Some(ZeroHourError::NoScience(science)) if science == "SCIENCE_Lost"
    ));
    // No ranks, no track and no perk.
    let mut none = rules();
    none.ranks = RankIni::default();
    let import = ScienceImport::of(&none, [].into_iter()).unwrap();
    assert!(import.track.is_none() && import.perks.is_empty());
}
