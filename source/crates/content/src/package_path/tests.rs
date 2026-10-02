use super::*;

#[test]
fn a_path_is_plain_names_joined_by_slashes_with_one_spelling() {
    for text in [
        "manifest.toml",
        "scripts/a.rhai",
        "data/units.toml",
        "a b/c-d.rhai",
    ] {
        assert_eq!(
            PackagePath::parse(text).map(|path| path.0),
            Some(text.to_owned())
        );
    }
    for text in [
        "",
        "/scripts/a.rhai",
        "scripts/",
        "scripts//a.rhai",
        "scripts/./a.rhai",
        "./scripts/a.rhai",
        "scripts/../a.rhai",
        "..",
        "scripts\\a.rhai",
        "C:\\scripts\\a.rhai",
    ] {
        assert_eq!(PackagePath::parse(text), None, "{text:?}");
    }
    let script = PackagePath::parse("scripts/ai/think.rhai").unwrap();
    assert!(script.is_under("scripts") && script.is_under("scripts/ai"));
    assert!(!script.is_under("script") && !script.is_under("scripts/ai/think.rhai"));
}
