use std::num::NonZeroU64;

use campfire_common::{DeclaredName, PlayerSlot, StableNumber};
use campfire_sim::StableId;
use campfire_values::Team;

use super::*;
use crate::probe_arg::ProbeUnitsArg;

#[test]
fn the_packages_go_where_the_line_says_or_to_the_home_directory() {
    let line = |args: &[&str]| CommandLine::try_parse_from(args).unwrap();
    let home = || Some(PathBuf::from("/h"));
    assert_eq!(
        line(&["check", "/out"]).out_dir(home()),
        Some(PathBuf::from("/out"))
    );
    assert_eq!(
        line(&["check"]).out_dir(home()),
        Some(PathBuf::from("/h/zh-package"))
    );
    assert_eq!(line(&["check"]).out_dir(None), None);
}

#[test]
fn a_probe_reads_its_play_period_and_units_and_each_names_a_play_it_plays_once() {
    let every = |ms| NonZeroU64::new(ms).unwrap();
    let id = |id| StableId::from(StableNumber::new(id));
    for (text, units) in [
        ("attack:1000:all", ProbeUnitsArg::All),
        // Players count from 1, as the check names them: player 2 is slot 1.
        (
            "attack:1000:slot=2",
            ProbeUnitsArg::Owner(PlayerSlot::new(1)),
        ),
        ("attack:1000:team=3", ProbeUnitsArg::Team(Team::new(3))),
        (
            "attack:1000:type=chinavehicledozer",
            ProbeUnitsArg::Type(DeclaredName::new("chinavehicledozer").unwrap()),
        ),
        (
            "attack:1000:ids=9,2",
            ProbeUnitsArg::Ids(vec![id(9), id(2)]),
        ),
    ] {
        let probe: ProbeArg = text.parse().unwrap();
        let expected = ProbeArg {
            play: Play::Attack,
            every_ms: every(1000),
            units,
        };
        assert_eq!(probe, expected, "{text}");
    }
    for text in [
        "attack:1000",
        "nowhere:1000:all",
        "attack:0:all",
        "attack:soon:all",
        "attack:1000:some",
        "attack:1000:slot=0",
        "attack:1000:team=x",
        "attack:1000:ids=1,,2",
        "attack:1000:type=",
    ] {
        assert!(text.parse::<ProbeArg>().is_err(), "{text}");
    }

    // A run of every play probes any; one of the base play alone probes it, not the attack play;
    // two probes of one play, or one of a play the run does not play, refuse the line.
    let line = |args: &[&str]| CommandLine::try_parse_from(args).unwrap();
    let all = line(&["check", "--probe", "attack:1000:all"])
        .selection()
        .unwrap();
    assert!(all.runs(Play::Base) && all.runs(Play::Attack));
    let probe = all.probe(Play::Attack).unwrap();
    assert_eq!(probe.file, PathBuf::from("target/zero-hour/attack.probe"));
    assert_eq!(all.probe(Play::Base), None);
    let base = line(&[
        "check",
        "--play",
        "base",
        "--probe",
        "base:500:all",
        "--probe-dir",
        "/p",
    ]);
    let base = base.selection().unwrap();
    assert!(base.runs(Play::Base) && !base.runs(Play::Attack));
    assert_eq!(
        base.probe(Play::Base).unwrap().file,
        PathBuf::from("/p/base.probe")
    );
    let twice = line(&[
        "check",
        "--probe",
        "base:500:all",
        "--probe",
        "base:100:all",
    ]);
    assert_eq!(
        twice.selection().unwrap_err(),
        SelectionError::Twice(Play::Base)
    );
    let unplayed = line(&["check", "--play", "attack", "--probe", "base:500:all"]);
    assert_eq!(
        unplayed.selection().unwrap_err(),
        SelectionError::Unplayed(Play::Base)
    );
}
