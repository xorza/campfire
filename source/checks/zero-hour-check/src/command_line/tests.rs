use campfire_capabilities::ProbeSpec;

use super::*;

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
    // The play, then the probe as the harness reads it (`ProbeSpec`, whose own test reads its
    // units).
    let probe: ProbeArg = "attack:1000:player=2".parse().unwrap();
    let spec: ProbeSpec = "1000:player=2".parse().unwrap();
    assert_eq!(
        probe,
        ProbeArg {
            play: Play::Attack,
            spec
        }
    );
    for text in [
        "attack",
        "nowhere:1000:all",
        "attack:1000",
        "attack:1000:some",
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
    assert_eq!(all.record(), PlayRecord::None);
    let recorded = line(&["check", "--record"]).selection().unwrap();
    assert_eq!(recorded.record(), PlayRecord::Kept);
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
