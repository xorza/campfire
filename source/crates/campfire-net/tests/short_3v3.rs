//! The MOBA 3v3, shortened for the tests that play it in process: its pick and first wave a
//! second each, and the heroes spawning 6 m apart in the jungle south of the north base, 20 m
//! and more from every structure and camp, so the heroes meet within a test's ticks.

use std::path::PathBuf;
use std::sync::Arc;

use campfire_common::MapName;
use campfire_package::{ModePackages, PackageDir};

/// The pick of each slot, the players' then the bots': a hero each, none twice, in tick 1, and
/// the spells `haste` and `mend` in tick 2.
pub(crate) const PICKS: [&str; 6] = [
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-cinder\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-gale\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-husk\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-kensho\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-rime\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
    "[[input]]\ntick = 1\nname = \"hero\"\nvalue = \"hero-veil\"\n\
     [[input]]\ntick = 2\nname = \"spells\"\nvalue = [\"haste\", \"mend\"]\n",
];

/// An edit of the copy: the first `from` of the file at `path`, from the MOBA packages' root,
/// becomes `to`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Edit {
    pub(crate) path: &'static str,
    pub(crate) from: &'static str,
    pub(crate) to: &'static str,
}

/// The short 3v3, then `edits` made, and `tail` after its mode's data, a `[visibility]` table or
/// nothing.
pub(crate) fn short_3v3(edits: &[Edit], tail: &str) -> ModePackages {
    let mut files = PackageDir::workspace_tree("test/moba");
    let mut edit = |path: &str, from: &str, to: &str| {
        let path = PathBuf::from(path);
        let text = String::from_utf8(files[&path].clone()).unwrap();
        assert!(text.contains(from), "{from}");
        files.insert(path, text.replacen(from, to, 1).into_bytes());
    };
    let data = "modes/3v3/data/mode.toml";
    edit(data, "pick_ms = 60000", "pick_ms = 1000");
    edit(data, "first_wave_ms = 60000", "first_wave_ms = 1000");
    let map = "modes/3v3/map/two_lanes/map.toml";
    for (team, from, to) in [("north", -60, -28), ("south", 60, -22)] {
        let marker = |z| format!("team = \"{team}\"\npos = [0, {z}]");
        edit(map, &marker(from), &marker(to));
    }
    for &Edit { path, from, to } in edits {
        edit(path, from, to);
    }
    let path = PathBuf::from(data);
    let mut text = String::from_utf8(files[&path].clone()).unwrap();
    text.push_str(tail);
    files.insert(path, text.into_bytes());
    let dir = PackageDir::in_memory(Arc::new(PackageDir::reindexed(files)), "modes/3v3");
    ModePackages::from_package_dir(&dir, &MapName::new("two_lanes").unwrap()).unwrap()
}
