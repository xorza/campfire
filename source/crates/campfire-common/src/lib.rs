//! The vocabulary that crates which do not depend on each other share: the player slot, ticks,
//! segment seeds and 32-byte values of the session log and the sim, and a package's fingerprint.
//!
//! A type enters only when two crates that do not depend on each other both name it, and only
//! as a plain value: construction, parsing, display and serde, no other logic. The crate
//! depends on nothing but `serde`.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]

mod bytes32;
mod fingerprint;
mod player_slot;
mod segment_seed;
mod tick;

pub use bytes32::Bytes32;
pub use bytes32::error::NotHex;
pub use fingerprint::Fingerprint;
pub use player_slot::PlayerSlot;
pub use segment_seed::SegmentSeed;
pub use tick::{Tick, Ticks};

#[cfg(test)]
mod tests {
    use std::fs;

    #[test]
    fn the_crate_depends_on_serde_alone() {
        // Read by hand, as the crate has no TOML parser. Only the listed sections may appear, so
        // a dependency in any other form, such as `[dependencies.name]`, a target's section or
        // a dotted key before the first section, fails the test instead of passing it unseen.
        let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
        let manifest = manifest.unwrap();
        let mut section = "";
        let mut names = Vec::new();
        for line in manifest.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[') {
                section = header.strip_suffix(']').unwrap();
                let known = ["package", "dependencies", "dev-dependencies", "lints"];
                assert!(known.contains(&section), "section [{section}]");
            } else {
                assert!(!section.is_empty(), "{line:?} before the first section");
                if section == "dependencies" {
                    names.push(line.split(['.', ' ', '=']).next().unwrap());
                }
            }
        }
        assert_eq!(names, ["serde"]);
    }

    #[test]
    fn the_design_marks_built_exactly_the_crates_of_the_workspace() {
        // Design 02's module table names each module in its first column and its status in the
        // second; a built module's crate is `campfire-<module>` in `crates/` or `checks/`.
        let source = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
        let design =
            fs::read_to_string(format!("{source}/../docs/design/02-engine-core.md")).unwrap();
        let table = design
            .lines()
            .skip_while(|line| *line != "| Module | Status | Does |")
            .skip(2)
            .take_while(|line| line.starts_with('|'));
        let mut built = Vec::new();
        for row in table {
            let cells: Vec<&str> = row.split('|').map(str::trim).collect();
            match cells[2] {
                "built" => built.push(format!("campfire-{}", cells[1].trim_matches('`'))),
                "planned" => {}
                status => panic!("{status:?} is no status, in {row}"),
            }
        }
        built.sort_unstable();
        let mut crates = Vec::new();
        for folder in ["crates", "checks"] {
            for entry in fs::read_dir(format!("{source}/{folder}")).unwrap() {
                let entry = entry.unwrap();
                if entry.path().join("Cargo.toml").is_file() {
                    crates.push(entry.file_name().into_string().unwrap());
                }
            }
        }
        crates.sort_unstable();
        assert_eq!(built, crates);
    }
}
