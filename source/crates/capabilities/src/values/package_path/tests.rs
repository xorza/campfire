use super::*;

#[test]
fn a_path_is_plain_names_joined_by_slashes_with_one_spelling() {
    for text in [
        "manifest.toml",
        "scripts/a.rhai",
        "data/units.toml",
        "a b/c-d.rhai",
        // Near a refused name, and a name every OS holds.
        "console.rhai",
        "com10.rhai",
        "lpt.rhai",
        "a.b.c",
        ".hidden",
        "scripts/null.rhai",
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
        // A name Windows cannot hold: each character it refuses, a control character, a dot or
        // a space at a name's end, and a device name, bare, with an extension, in any case,
        // before a space, and in a directory's place.
        "a:b.rhai",
        "a*b.rhai",
        "a?b.rhai",
        "a\"b.rhai",
        "a<b.rhai",
        "a>b.rhai",
        "a|b.rhai",
        "a\tb.rhai",
        "a\u{7f}b.rhai",
        // A name past ASCII, which an OS may fold or normalize: a composed and a decomposed é.
        "h\u{e9}ros.rhai",
        "he\u{301}ros.rhai",
        "a\u{a0}b.rhai",
        "scripts/a.",
        "scripts/a ",
        "scripts./a.rhai",
        "con",
        "CON.rhai",
        "Aux.toml",
        "nul.tar.gz",
        "com1",
        "COM\u{b9}.rhai",
        "lpt9.rhai",
        "prn .rhai",
        "con/a.rhai",
    ] {
        assert_eq!(PackagePath::parse(text), None, "{text:?}");
    }
    let script = PackagePath::parse("scripts/ai/think.rhai").unwrap();
    assert!(script.is_under("scripts") && script.is_under("scripts/ai"));
    assert!(!script.is_under("script") && !script.is_under("scripts/ai/think.rhai"));
}
