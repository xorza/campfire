//! A snapshot restores each state type by its layout: a type whose layout changes at the same
//! `StateRegistry::DATA_VERSION` would read an older snapshot's bytes as other values, so its
//! fingerprint, pinned with the version, fails here until the version is raised.

use std::collections::BTreeMap;
use std::env;
use std::fmt::Write as _;
use std::path::PathBuf;

use campfire_log::ErrorReport;
use campfire_runner::Session;
use campfire_runner::internals::ProvingMatch;
use campfire_sim::StateRegistry;
use campfire_store::{DurableFile, InputFile};

/// The hex digits of each fingerprint the file pins.
const DIGITS: usize = 16;
/// The most bytes the file may hold: a line of about 40 bytes for each state type.
const MAX_LEN: usize = 1 << 16;

/// Each line of a layout file but its version's, by type name.
fn by_name(text: &str) -> BTreeMap<&str, &str> {
    text.lines()
        .skip(1)
        .filter_map(|line| line.split_once(' '))
        .collect()
}

/// The names of the types `pinned` and `now` differ in: changed, added or gone.
fn differing<'a>(pinned: &'a str, now: &'a str) -> Vec<&'a str> {
    let (pinned, now) = (by_name(pinned), by_name(now));
    let mut names: Vec<&str> = pinned.keys().chain(now.keys()).copied().collect();
    names.sort_unstable();
    names.dedup();
    names.retain(|name| pinned.get(name) != now.get(name));
    names
}

#[test]
fn a_state_types_layout_changes_only_with_the_snapshot_version() {
    // The proving match holds every type of every capability the release runs, and the mode's.
    let proving = ProvingMatch::load();
    let fixed = proving.start();
    let registry = fixed.runner().world().resource::<Session>().registry();
    let mut now = format!("data_version {}\n", StateRegistry::DATA_VERSION);
    for (name, fingerprint) in registry.layout() {
        writeln!(now, "{name} {}", &fingerprint.to_string()[..DIGITS])
            .expect("text writes into a string");
    }
    let path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/layout.txt"
    ));
    let pinned = InputFile::read_text(&path, MAX_LEN).ok();
    let same_version = |pinned: &str| pinned.lines().next() == now.lines().next();
    if let Some(pinned) = &pinned
        && *pinned != now
        && same_version(pinned)
    {
        panic!(
            "the layout of {:?} changed at data version {}: raise `StateRegistry::DATA_VERSION`, \
             then write the file again with CAMPFIRE_BLESS=1",
            differing(pinned, &now),
            StateRegistry::DATA_VERSION
        );
    }
    if env::var_os("CAMPFIRE_BLESS").is_some() {
        DurableFile::write(&path, now.as_bytes())
            .unwrap_or_else(|error| panic!("{}", ErrorReport::of(&error)));
        return;
    }
    assert_eq!(
        pinned.as_deref(),
        Some(now.as_str()),
        "the layouts pinned for another data version, or none: write them with CAMPFIRE_BLESS=1"
    );
}

#[test]
fn the_layout_check_names_each_type_that_differs() {
    // A type whose fingerprint changed, one added and one gone; the one that stayed is not named.
    let pinned = "data_version 2\na 0001\nb 0002\nc 0003\n";
    let now = "data_version 2\na 0001\nb 0009\nd 0004\n";
    assert_eq!(differing(pinned, now), ["b", "c", "d"]);
}
