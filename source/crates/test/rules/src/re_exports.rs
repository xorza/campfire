use crate::manifests::COMMON_DERIVE;
use crate::workspace::sources;

/// The lines of `text` that re-export another crate, whole or in part: a `pub use`, of any
/// visibility, whose path starts with a name other than `crate`, `$crate`, `self` or `super`, and
/// a `pub extern crate`.
fn foreign_re_exports(text: &str) -> Vec<usize> {
    fn root(path: &str) -> &str {
        let path = path.trim_start_matches("::");
        let end = path
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(path.len());
        &path[..end]
    }
    text.lines()
        .enumerate()
        .filter(|(_, line)| {
            let Some(rest) = line.trim_start().strip_prefix("pub") else {
                return false;
            };
            let rest = match rest.strip_prefix('(') {
                Some(scoped) => scoped.split_once(')').map_or("", |(_, rest)| rest),
                None => rest,
            };
            let rest = rest.trim_start();
            rest.starts_with("extern crate ")
                || rest
                    .strip_prefix("use ")
                    .is_some_and(|path| !["crate", "$crate", "self", "super"].contains(&root(path)))
        })
        .map(|(at, _)| at + 1)
        .collect()
}

/// Whether `line` of the file at `path` is the one re-export of another crate the rule allows:
/// `common`'s of the derives of its own traits, from the proc-macro crate that alone may hold
/// them, which no other crate names (02-engine-core.md, Structural rules).
fn derives_of_common(path: &str, line: &str) -> bool {
    let from = format!("pub use {}::", COMMON_DERIVE.replace('-', "_"));
    path == "crates/base/common/src/lib.rs" && line.trim_start().starts_with(&from)
}

#[test]
fn no_crate_re_exports_another_crate() {
    let found: Vec<String> = sources()
        .iter()
        .flat_map(|file| {
            let lines: Vec<&str> = file.text.lines().collect();
            foreign_re_exports(&file.text)
                .into_iter()
                .filter(move |&line| !derives_of_common(&file.path, lines[line - 1]))
                .map(move |line| format!("{}:{line}", file.path))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // A whole crate, an item, a scoped one, an indented group and an extern crate are found; the
    // crate's own, a macro's own and a plain import are not.
    let sample = "pub use rhai;\npub use crate::a::B;\npub(crate) use ::toml::de::Error as E;\n    pub use other::{A, B};\npub use self::x;\nuse other::C;\npub use super::y;\npub extern crate rhai;\n        pub use $crate::Item;";
    assert_eq!(foreign_re_exports(sample), [1, 3, 4, 8]);
}
