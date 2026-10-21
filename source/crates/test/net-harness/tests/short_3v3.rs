//! The MOBA 3v3, shortened for the tests that play it in process: its pick and first wave a
//! second each, and the heroes spawning 6 m apart in the jungle south of the north base, 20 m
//! and more from every structure and camp, so the heroes meet within a test's ticks.

use campfire_package::ModePackages;
use campfire_test_packages::{Edit, PackageTree, TestMode};

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

/// The short 3v3, then `edits` made, each to a file by its path from the MOBA packages' root, and
/// `tail` after its mode's data, a `[visibility]` table or nothing.
pub(crate) fn short_3v3(edits: &[(&str, Edit<'_>)], tail: &str) -> ModePackages {
    let mut tree = PackageTree::of(TestMode::Moba3v3);
    let data = "modes/3v3/data/mode.toml";
    tree.edit(data, Edit::Replace("pick_ms = 60000", "pick_ms = 1000"))
        .edit(
            data,
            Edit::Replace("first_wave_ms = 60000", "first_wave_ms = 1000"),
        );
    let map = "modes/3v3/map/two_lanes/map.toml";
    for (team, from, to) in [("north", -60, -28), ("south", 60, -22)] {
        let marker = |z| format!("team = \"{team}\"\nposition = [0, {z}]");
        tree.edit(map, Edit::Replace(&marker(from), &marker(to)));
    }
    tree.edit_all(edits.iter().copied());
    tree.edit(data, Edit::Append(tail)).packages()
}
