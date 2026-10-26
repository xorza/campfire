use campfire_common::{MapName, PlayerSlot, Tick};
use campfire_protocol::SessionId;
use campfire_sim::StableId;
use campfire_store::internals::TestDir;
use campfire_values::PackageName;

use super::*;
use crate::client::client_layout::{ClientLayout, NewReport};
use crate::client::report_file::ReportCamera;
use crate::messages::journal_command::ReportKey;

#[test]
fn a_report_s_facts_and_screenshot_read_back_from_its_folder() {
    let test_dir = TestDir::new();
    let session = SessionId::new([9; 32]);
    let layout = ClientLayout::at(test_dir.path("client"));
    // Each report of one tick gets a folder of its own, as a paused match draws one tick: the
    // first by the tick alone, a later one by its take after it.
    let NewReport { dir, key } = layout.create_report_dir(session, Tick::new(1234)).unwrap();
    let again = layout.create_report_dir(session, Tick::new(1234)).unwrap();
    let reports = test_dir.path("client/reports");
    assert_eq!(dir.path(), reports.join(format!("{session}-1234")));
    assert_eq!(again.dir.path(), reports.join(format!("{session}-1234-1")));
    let tick = Tick::new(1234);
    assert_eq!(key, ReportKey { tick, take: 0 });
    assert_eq!(again.key, ReportKey { tick, take: 1 });
    assert_eq!(layout.report_dir(session, again.key), again.dir);
    // Floats of no short spelling, as a rig's yaw is: each reads back to its own bits.
    let facts = ReportFile {
        release: Version::ENGINE,
        mode: PackageName::new("zero-hour").unwrap(),
        mode_dir: PathBuf::from("/home/player/zh-package/game"),
        map: MapName::parse("tournament_desert").unwrap(),
        session,
        tick: Tick::new(1234),
        player_slot: Some(PlayerSlot::new(1)),
        selected: vec![StableId::of_player(PlayerSlot::new(1))],
        camera: ReportCamera {
            pivot: [100.25, -50.0],
            yaw: 1.0 / 3.0,
            pitch: 37.5_f32.to_radians(),
            height: 310.0,
            place: [100.0, 330.0, 353.999_9],
            forward: [0.1, -0.608_761_4, -0.793_353_3],
        },
        window: [7680, 2160],
    };
    dir.write_facts(&facts).unwrap();
    let read = dir.read_facts().unwrap();
    assert_eq!(read, facts);
    assert_eq!(read.camera.yaw.to_bits(), (1.0_f32 / 3.0).to_bits());
    // A spectator's report holds no player slot.
    let watched = ReportFile {
        player_slot: None,
        ..facts
    };
    dir.write_facts(&watched).unwrap();
    let read = dir.read_facts().unwrap();
    assert_eq!(read.player_slot, None);
    // A report of another release is refused before its packages load.
    let other = Version::new(0, 0, 1);
    dir.write_facts(&ReportFile {
        release: other,
        ..watched.clone()
    })
    .unwrap();
    assert!(
        matches!(dir.read_facts(), Err(ReportReadError::OtherRelease(release)) if release == other)
    );
    dir.write_facts(&watched).unwrap();
    // One red pixel over one white one seen half through.
    let screenshot = RgbaImage {
        width: 1,
        height: 2,
        pixels: vec![255, 0, 0, 255, 255, 255, 255, 128],
    };
    dir.write_screenshot(&screenshot).unwrap();
    let file = InputFile::read(&dir.screenshot(), 1 << 10).unwrap();
    assert_eq!(Png::decode(&file).unwrap(), screenshot);
}
