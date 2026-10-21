use super::*;

#[test]
fn a_dependency_path_names_only_directories_and_their_parents() {
    let path = DependencyPath::parse("../../heroes/lancer").unwrap();
    assert_eq!(
        path.names().collect::<Vec<_>>(),
        ["..", "..", "heroes", "lancer"]
    );
    assert!(DependencyPath::parse("rules").is_some());
    // A root, a drive, a share, a backslash, an empty name, `.`, a device and a name Windows
    // drops the end of: each a way out of the workspace or another directory on another OS.
    for refused in [
        "/rules",
        "C:/rules",
        "//host/share",
        "..\\rules",
        "a//b",
        "",
        "./rules",
        "../aux",
        "rules.",
    ] {
        assert_eq!(DependencyPath::parse(refused), None, "{refused:?}");
    }
}
