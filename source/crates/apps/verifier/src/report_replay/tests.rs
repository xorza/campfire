use campfire_common::{Tick, Version};
use campfire_harness::FixedSession;
use campfire_host::{ClientLayout, ReportCamera};
use campfire_runner::SessionWorld;
use campfire_store::internals::TestDir;
use campfire_test_packages::TestMode;

use super::*;

#[test]
fn a_report_tables_the_units_its_match_held_at_its_tick() {
    let mode = TestMode::Lane;
    let mut fixed = FixedSession::new(mode.packages(), 1).start();
    // The tables of the match as a client drew it after its ticks 0 to 39, of every unit and of
    // the first team's: a report of tick 39. The match then runs on through tick 59 before the
    // server publishes its log.
    for _ in 0..40 {
        fixed.step();
    }
    let mut drawn = MatchProbe::new(NonZeroU64::MIN, ProbeUnits::All);
    drawn.table(fixed.game());
    let spec: ProbeSpec = "1000:team=0".parse().unwrap();
    let game = fixed.game();
    let types = game.get_resource::<UnitTypeBook>();
    let mut drawn_team = spec
        .probe(types, game.resource::<MatchRate>().hz())
        .unwrap();
    drawn_team.table(game);
    for _ in 0..20 {
        fixed.step();
    }
    let game = fixed.game_mut();
    game.session_mut().reveal_seed();
    let mut log = Vec::new();
    game.session().log().encode(&mut log);
    let session = game.session().log().session_id();
    let test_dir = TestDir::new();
    let layout = ClientLayout::at(test_dir.path("client"));
    let dir = layout
        .create_report_dir(session, Tick::new(39))
        .unwrap()
        .dir;
    let facts = ReportFile {
        release: Version::ENGINE,
        mode: mode.packages().manifest().header.name.clone(),
        mode_dir: mode.dir(),
        map: mode.map(),
        session,
        tick: Tick::new(39),
        player_slot: None,
        selected: Vec::new(),
        camera: ReportCamera {
            pivot: [0.0; 2],
            yaw: 0.0,
            pitch: 1.0,
            height: 24.0,
            place: [0.0; 3],
            forward: [0.0; 3],
        },
        window: [1920, 1080],
    };
    dir.write_facts(&facts).unwrap();
    DurableFile::write(&dir.log(), &log).unwrap();

    // Its replay tables what the client drew, the units the match held.
    let mut out = Vec::new();
    replay_report(dir.path(), None, &mut out).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), drawn.text());
    let heading = drawn.text().lines().next();
    assert_eq!(heading, Some("tick 39, 1 s and 9 ticks"));
    assert!(drawn.text().lines().count() > 3, "{}", drawn.text());

    // A probe of a second at the lane's 30 ticks a second tables after ticks 0 and 30 in its
    // file, and the table at the tick takes its units, of the first team alone.
    let probe = ReportProbe {
        spec,
        dir: test_dir.path("probes"),
    };
    let mut out = Vec::new();
    replay_report(dir.path(), Some(&probe), &mut out).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), drawn_team.text());
    assert_ne!(drawn_team.text(), drawn.text());
    let name = format!("{}-39", &session.to_string()[..8]);
    let periods = test_dir.read(format!("probes/{name}.probe"));
    let periods = String::from_utf8(periods).unwrap();
    let headings: Vec<&str> = (periods.lines())
        .filter(|line| line.starts_with("tick"))
        .collect();
    assert_eq!(headings, ["tick 0, 0 s", "tick 30, 1 s"]);

    // A report of a tick its log has not run does not replay: the log ran ticks 0 to 59.
    let past = ReportFile {
        tick: Tick::new(60),
        ..facts
    };
    dir.write_facts(&past).unwrap();
    let refused = ReportReplay::open(&dir).unwrap().tables(None);
    assert!(matches!(
        refused,
        Err(ReportReplayError::PastLog { tick, end })
            if (tick, end) == (Tick::new(60), Tick::new(60))
    ));
}
