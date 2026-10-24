use crate::named_path::NamedPath;
use crate::workspace::Workspace;

/// The one crate whose code may differ by OS (02-engine-core.md, Platform).
const PLATFORM: &str = "crates/base/platform/src/";

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

/// The modules of std's `os` that one OS family alone has, which code names with no `std` before
/// them once a `use` brings `os`.
const OS_MODULES: [&str; 6] = ["unix", "windows", "fd", "linux", "macos", "wasi"];

/// The lines of `text` that name an OS: a `cfg`, `cfg!` or `cfg_attr` whose condition holds a
/// word of `OS_WORDS`, at the condition's line, or a path through std's or core's `os` module, or
/// through `os` and one of its `OS_MODULES`. A module of the workspace that is named `os` is no
/// OS's.
fn os_names(text: &str) -> Vec<usize> {
    // A file of no path through `os` and of no word of an OS names none, and its parse is
    // skipped.
    let os = text.contains("os::") || text.contains("::os");
    if !os && !OS_WORDS.iter().any(|word| text.contains(word)) {
        return Vec::new();
    }
    let mut lines: Vec<usize> = NamedPath::of(text)
        .into_iter()
        .filter_map(|path| {
            if let Some(cfg) = path.cfg {
                return OS_WORDS.contains(&path.text.as_str()).then_some(cfg);
            }
            let crate_os = ["std::os::", "core::os::"]
                .iter()
                .any(|os| path.text.starts_with(os));
            let module = (path.text.strip_prefix("os::"))
                .map(|rest| rest.split("::").next().expect("a split holds a part"));
            let grouped = module.is_some_and(|module| OS_MODULES.contains(&module));
            (crate_os || grouped).then_some(path.line)
        })
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

pub(crate) fn only_the_platform_module_names_an_os(workspace: &Workspace) {
    let sources = workspace.sources();
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
    // The crate names one, so the walk reaches it and the check sees what it holds.
    let mut platform = sources
        .iter()
        .filter(|file| file.path.starts_with(PLATFORM) && !os_names(&file.text).is_empty())
        .map(|file| file.path.as_str());
    assert!(platform.any(|path| path == "crates/base/platform/src/lib.rs"));
}

#[test]
fn the_platform_rule_check_finds_each_way_to_name_an_os() {
    let (cfg, os) = ("cfg", "std::os");
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
        (
            "use std::{\n    fs,\n    os::unix::fs::PermissionsExt,\n};\n".to_owned(),
            vec![3],
        ),
        (format!("#[{cfg}(target_vendor = \"apple\")]\n"), vec![1]),
        (
            format!("#[{cfg}(not(any(unix, windows)))]\n#[{cfg}(unix)]\n"),
            vec![1, 2],
        ),
        // No OS: another condition, a word that only holds `cfg`, an OS in a comment or a string,
        // and a module of the workspace named `os`.
        (
            format!("#[{cfg}(test)]\n#[{cfg}(feature = \"internals\")]\n"),
            vec![],
        ),
        (
            format!(
                "let config = unix_time();\n// on windows\nlet s = \"std::os::unix\";\npub use crate::{}::Os;\n",
                "os"
            ),
            vec![],
        ),
    ] {
        assert_eq!(os_names(&text), lines, "{text}");
    }
}
