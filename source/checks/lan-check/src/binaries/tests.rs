use campfire_codec::{Toml, TomlTable};

use super::*;

#[test]
fn the_driver_builds_the_check_and_each_binary_it_runs() {
    let mut lan: TomlTable = Toml::parse(include_str!("../../../../Cargo.toml")).unwrap();
    for key in ["workspace", "metadata", "drive", "commands", "lan"] {
        lan = lan
            .take(key)
            .expect("the drive table holds the `lan` command")
            .unwrap();
    }
    let mut steps: Vec<TomlTable> = lan.take("steps").unwrap().unwrap();
    let build: String = steps[0]
        .take("build")
        .expect("the `lan` command builds first")
        .unwrap();
    let words: Vec<&str> = build.split_whitespace().collect();
    let mut built: Vec<&str> = words
        .windows(2)
        .filter(|pair| pair[0] == "-p")
        .map(|pair| pair[1])
        .collect();
    built.sort_unstable();
    let mut run = vec!["campfire-lan-check"];
    run.extend(Binaries::NAMES);
    run.sort_unstable();
    assert_eq!(built, run);
}
