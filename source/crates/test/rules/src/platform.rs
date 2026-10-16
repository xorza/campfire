use crate::workspace::sources;

/// The one module whose code may differ by OS (02-engine-core.md, Platform).
const PLATFORM: &str = "crates/base/store/src/platform/";

/// The words of a condition that name an OS, or the vendor or the toolchain environment that
/// stand for one, which only `PLATFORM` may test.
const OS_WORDS: [&str; 6] = [
    "unix",
    "windows",
    "target_os",
    "target_family",
    "target_vendor",
    "target_env",
];

/// The modules of std's `os` that one OS family alone has, which a grouped import names with no
/// `std` before them.
const OS_MODULES: [&str; 6] = ["unix", "windows", "fd", "linux", "macos", "wasi"];

/// The lines of `text` that name an OS: a `cfg`, `cfg!` or `cfg_attr` whose condition holds a
/// word of `OS_WORDS`, or a path through std's or core's `os` module, or through one of its
/// `OS_MODULES` as a group gives it. A module of the workspace that is named `os` is no OS's.
/// The patterns are spelled in parts, so that this file's own samples hold none of them.
fn os_names(text: &str) -> Vec<usize> {
    let line_of = |at: usize| text[..at].matches('\n').count() + 1;
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let os = concat!("os", "::");
    let mut lines: Vec<usize> = text
        .match_indices(os)
        .filter(|&(at, _)| {
            let before = &text[..at];
            let rest = &text[at + os.len()..];
            let crate_os = ["std::", "core::"]
                .iter()
                .any(|path| before.ends_with(path));
            let grouped = !before.ends_with(word)
                && !before.ends_with("::")
                && OS_MODULES.iter().any(|module| {
                    rest.strip_prefix(module)
                        .is_some_and(|after| !after.starts_with(word))
                });
            crate_os || grouped
        })
        .map(|(at, _)| line_of(at))
        .collect();
    for (at, _) in text.match_indices("cfg") {
        if text[..at].ends_with(word) {
            continue;
        }
        let rest = &text[at + 3..];
        let rest = rest
            .strip_prefix("_attr")
            .or_else(|| rest.strip_prefix('!'))
            .unwrap_or(rest);
        let Some(rest) = rest.strip_prefix('(') else {
            continue;
        };
        let mut depth = 1;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
            .map_or(rest.len(), |(end, _)| end);
        if rest[..end]
            .split(|c: char| !word(c))
            .any(|name| OS_WORDS.contains(&name))
        {
            lines.push(line_of(at));
        }
    }
    lines.sort_unstable();
    lines.dedup();
    lines
}

#[test]
fn only_the_platform_module_names_an_os() {
    let sources = sources();
    let named: Vec<String> = sources
        .iter()
        .filter(|file| !file.path.starts_with(PLATFORM))
        .flat_map(|file| {
            os_names(&file.text)
                .into_iter()
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(named, Vec::<String>::new());
    // The module names one, so the walk reaches it and the check sees what it holds.
    let mut platform = sources
        .iter()
        .filter(|file| file.path.starts_with(PLATFORM) && !os_names(&file.text).is_empty())
        .map(|file| file.path.as_str());
    assert!(platform.any(|path| path == "crates/base/store/src/platform/mod.rs"));
}

#[test]
fn the_platform_rule_check_finds_each_way_to_name_an_os() {
    let (cfg, os) = ("cfg", concat!("std", "::os"));
    for (text, lines) in [
        (format!("#[{cfg}(unix)]\nfn f() {{}}\n"), vec![1]),
        (format!("//! x\n#[{cfg}(all(test, windows))]\n"), vec![2]),
        (format!("let x = {cfg}!(target_os = \"macos\");\n"), vec![1]),
        (
            format!("#[{cfg}_attr(target_family = \"wasm\", path = \"w.rs\")]\n"),
            vec![1],
        ),
        (format!("use {os}::unix::fs;\n"), vec![1]),
        (format!("use std::{{fs, {}::windows}};\n", "os"), vec![1]),
        (format!("#[{cfg}(target_vendor = \"apple\")]\n"), vec![1]),
        (
            format!("#[{cfg}(not(any(unix, windows)))]\n#[{cfg}(unix)]\n"),
            vec![1, 2],
        ),
        // No OS: another condition, a word that only holds `cfg`, and an OS in a comment.
        (
            format!("#[{cfg}(test)]\n#[{cfg}(feature = \"internals\")]\n"),
            vec![],
        ),
        (
            format!(
                "let config = unix_time();\n// on windows\npub use crate::{}::Os;\n",
                "os"
            ),
            vec![],
        ),
    ] {
        assert_eq!(os_names(&text), lines, "{text}");
    }
}
