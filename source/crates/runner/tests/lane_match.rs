//! The test lane mode, with no player input, plays 30 s to its golden record, whatever its
//! heroes' text says.

use std::collections::BTreeMap;
use std::fs;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use campfire_capabilities::{ScriptBook, ScriptFailures};
use campfire_package::{ModePackages, PackageDir};
use campfire_runner::{FixedSession, Golden};
use campfire_sim::StateHash;

const LANE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packages/test/modes/lane"
);

#[test]
fn the_lane_match_plays_to_its_golden_record() {
    let packages =
        ModePackages::from_dir(Path::new(LANE)).unwrap_or_else(|error| panic!("{error}"));
    let session = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 2);
    let mut golden = Golden::new(session.packages(), session.players());
    let scripts = session.packages().script_book();
    let mut fixed = session.start();
    // The load's book of each script's hooks is the one the match fills as it compiles.
    let held = fixed.runner().world().resource::<ScriptBook>();
    assert_eq!(*held, scripts);
    for tick in 0..900 {
        fixed.runner_mut().run_tick();
        golden.record(fixed.runner());
        let failures = fixed.runner().world().non_send::<ScriptFailures>();
        assert!(
            failures.get().is_empty(),
            "tick {tick}: {:?}",
            failures.get()
        );
    }
    golden.check("lane");
}

#[test]
fn the_lane_matchs_hashes_do_not_change_with_its_heroes_text() {
    // Walker's name in other words, and in a second language: its package's fingerprint moves,
    // and no tick's hash does, as the sim reads no text.
    let tree = Path::new(LANE).join("../..");
    let mut files = BTreeMap::new();
    read_tree(&tree, Path::new(""), &mut files);
    let plain = PackageDir::in_memory(Arc::new(files.clone()), "modes/lane");
    files.insert(
        "heroes/walker/locale/en.ftl".into(),
        b"hero-name = Wanderer\n".to_vec(),
    );
    files.insert(
        "heroes/walker/locale/de.ftl".into(),
        b"hero-name = Wanderer\n".to_vec(),
    );
    let reworded = PackageDir::in_memory(Arc::new(files), "modes/lane");
    let [plain, reworded] =
        [plain, reworded].map(|dir| ModePackages::from_package_dir(&dir).unwrap());
    let walker = |packages: &ModePackages| {
        let walker = packages
            .packages()
            .find(|view| view.package.name == "hero-walker");
        walker.unwrap().package.fingerprint
    };
    assert_ne!(walker(&plain), walker(&reworded));
    assert_eq!(hashes(plain), hashes(reworded));
}

/// The state hash of each of the first 300 ticks of a lane match of `packages`.
fn hashes(packages: ModePackages) -> Vec<StateHash> {
    let session = FixedSession::new(packages, NonZeroU32::new(30).unwrap(), 2);
    let mut fixed = session.start();
    (0..300)
        .map(|_| {
            fixed.runner_mut().run_tick();
            fixed.runner().state_hash()
        })
        .collect()
}

fn read_tree(dir: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = at.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            read_tree(&entry.path(), &path, files);
        } else {
            files.insert(path, fs::read(entry.path()).unwrap());
        }
    }
}
