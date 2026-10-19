use std::collections::BTreeMap;

use campfire_store::internals::SourceFile;

use crate::workspace::{crates, holder, sources};

/// A function whose first parameter is a world: its line, its name with its owner's, as
/// `Owner::name`, or alone for a free function, and whether code past its module may call it.
#[derive(Debug, PartialEq, Eq)]
struct WorldFirst {
    line: usize,
    name: String,
    visible: bool,
}

/// The end of the bracketed run that opens at `text[at]`, past its closing bracket.
fn bracket_end(text: &str, at: usize, open: char, close: char) -> usize {
    let mut depth = 0;
    for (offset, c) in text[at..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return at + offset + 1;
            }
        }
    }
    text.len()
}

/// The type an `impl` line of `line` gives methods to, none for a trait's impl.
fn impl_self_type(line: &str) -> Option<&str> {
    let mut rest = line.trim_start().strip_prefix("impl")?;
    if rest.starts_with('<') {
        rest = &rest[bracket_end(rest, 0, '<', '>')..];
    }
    let rest = rest.trim_start();
    if rest.contains(" for ") {
        return None;
    }
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    Some(&rest[..end])
}

/// Each function of `text` whose first parameter is a world, a reference to a `World`.
fn world_first(text: &str) -> Vec<WorldFirst> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (at, _) in text.match_indices("fn ") {
        let line_start = text[..at].rfind('\n').map_or(0, |end| end + 1);
        let head = &text[line_start..at];
        let modifiers = head.trim_start();
        let words = modifiers
            .split_whitespace()
            .all(|word| word.starts_with("pub") || word == "const");
        if !words || text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue;
        }
        let rest = &text[at + 3..];
        let name_end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        let mut params = at + 3 + name_end;
        if text[params..].starts_with('<') {
            params = bracket_end(text, params, '<', '>');
        }
        if !text[params..].starts_with('(') {
            continue;
        }
        let close = bracket_end(text, params, '(', ')') - 1;
        let list = &text[params + 1..close];
        let first = list.split(',').next().unwrap_or("");
        let Some((_, kind)) = first.split_once(':') else {
            continue;
        };
        let Some(referent) = kind.trim().strip_prefix('&') else {
            continue;
        };
        let referent = referent.trim_start();
        let referent = referent.strip_prefix('\'').map_or(referent, |lifetime| {
            lifetime
                .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
                .trim_start()
        });
        let referent = referent
            .strip_prefix("mut ")
            .map_or(referent, str::trim_start);
        if referent != "World" {
            continue;
        }
        let line = text[..at].matches('\n').count();
        let indent = head.len() - modifiers.len();
        let owner = lines[..line].iter().rev().find_map(|above| {
            let code = above.trim_start();
            let at_impl = indent >= 4 && above.len() - code.len() == indent - 4;
            (at_impl && code.starts_with("impl")).then(|| impl_self_type(above))
        });
        let name = match owner {
            Some(None) => continue,
            Some(Some(owner)) => format!("{owner}::{name}"),
            None => name.to_owned(),
        };
        let visible = modifiers.starts_with("pub") && !modifiers.starts_with("pub(super)");
        found.push(WorldFirst {
            line: line + 1,
            name,
            visible,
        });
    }
    found
}

/// Whether `text` names the function `name`, as `Owner::name` or alone, as a value: Bevy
/// registers a system, an observer, a hook or a run condition by value, and a table holds a
/// function so.
fn named_as_value(text: &str, name: &str) -> bool {
    text.match_indices(name).any(|(at, _)| {
        let before = text[..at].trim_end();
        let after = text[at + name.len()..].trim_start();
        let whole = !text[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == ':')
            && !text[at + name.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_');
        let called = after.starts_with(['(', ':', '<']);
        let listed = before.ends_with(['(', ',', '[']) || name.contains("::");
        let line = &text[text[..at].rfind('\n').map_or(0, |end| end + 1)..at];
        let comment = line.trim_start().starts_with("//");
        whole && !called && listed && !comment
    })
}

#[test]
fn a_free_function_that_takes_a_world_first_is_a_system() {
    // The world rule (02-engine-core.md, Structural rules): what acts on the match's content is a
    // function of its module's type, and a value made from the world a constructor of its own. A
    // free function past its module that takes the world first is a system or a function a table
    // holds, named as a value.
    let sources = sources();
    let production: Vec<(&str, String)> = sources
        .iter()
        .filter_map(|file| Some((file.path.as_str(), file.production()?)))
        .collect();
    let mut found = Vec::new();
    for (path, code) in &production {
        let free = world_first(code)
            .into_iter()
            .filter(|f| f.visible && !f.name.contains("::"));
        for function in free {
            let valued = production
                .iter()
                .any(|(_, text)| named_as_value(text, &function.name));
            if !valued {
                found.push(format!("{path}:{} {}", function.line, function.name));
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

/// Each trait `text` declares that the world rule refuses, by its line and name: a test trait
/// on the world, `<Module>WorldTest`; a crate's internals trait on it,
/// `<Module>WorldCrateInternals`; and a module's trait on `Match`, `<Module>Match` or
/// `<Module>MatchInternals`, as a match a module builds comes from the module's type.
fn refused_traits(text: &str) -> Vec<(usize, &str)> {
    const REFUSED: [&str; 4] = [
        "WorldTest",
        "WorldCrateInternals",
        "Match",
        "MatchInternals",
    ];
    let mut found = Vec::new();
    for (line, code) in text.lines().enumerate() {
        let mut code = code.trim_start();
        if let Some(rest) = code.strip_prefix("pub") {
            let rest = rest.strip_prefix('(').map_or(rest, |inner| {
                inner.split_once(')').map_or(inner, |(_, after)| after)
            });
            code = rest.trim_start();
        }
        let Some(rest) = code.strip_prefix("trait ") else {
            continue;
        };
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let name = &rest[..end];
        let refused = REFUSED
            .iter()
            .any(|suffix| name.len() > suffix.len() && name.ends_with(suffix));
        if refused {
            found.push((line + 1, name));
        }
    }
    found
}

#[test]
fn no_trait_is_a_test_trait_on_the_world_or_a_modules_trait_on_a_match() {
    // The world rule (02-engine-core.md, Structural rules): a world trait holds the whole match and
    // the fast access to a module's systems, for other crates, its own crate, or the tests and
    // benches of any crate; a test's verbs belong to its fixture or its capability's type, and a
    // match a module builds to the module's type. Test code counts, as these traits lived in it;
    // this file holds a source of each form for the check's own test.
    let mut found = Vec::new();
    let own = "crates/test/rules/src/world.rs";
    for file in sources().into_iter().filter(|file| file.path != own) {
        for (line, name) in refused_traits(&file.text) {
            found.push(format!("{}:{line} {name}", file.path));
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_world_rule_finds_each_trait_it_refuses() {
    // Each refused form, at any visibility; the world traits it keeps, a trait named `Match`
    // alone and a type of such a name are none.
    let text = "pub trait UnitsWorldTest {}
\
        pub(crate) trait ActionsWorldCrateInternals {}
\
        \x20   trait SessionMatch: Sized {}
\
        pub(super) trait HarnessMatchInternals {}
\
        pub trait SimWorld {}
pub(crate) trait PlayersWorldCrate {}
\
        pub trait SimWorldInternals {}
trait Match {}
pub struct ScriptedMatch;
";
    assert_eq!(
        refused_traits(text),
        [
            (1, "UnitsWorldTest"),
            (2, "ActionsWorldCrateInternals"),
            (3, "SessionMatch"),
            (4, "HarnessMatchInternals"),
        ]
    );
}

/// Each use of a `<Module>WorldCrate` trait outside the crate that declares it, among `files`, each
/// a source by its crate's name, its path and its text: by the path, the line and the trait. Such a
/// trait is its own crate's alone (02-engine-core.md, Structural rules): another crate takes the
/// module's `<Module>World`, and a test or a bench its `<Module>WorldInternals`.
fn foreign_crate_traits<'a>(
    files: &[(&'a str, &'a str, &'a str)],
) -> Vec<(&'a str, usize, &'a str)> {
    let identifiers = |line: &'a str| {
        line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty())
    };
    let crate_trait = |name: &str| name.len() > "WorldCrate".len() && name.ends_with("WorldCrate");
    let mut declared = BTreeMap::new();
    for &(owner, _, text) in files {
        for line in text.lines() {
            let mut words = identifiers(line);
            while let Some(word) = words.next() {
                if word == "trait"
                    && let Some(name) = words.next()
                    && crate_trait(name)
                {
                    declared.insert(name, owner);
                }
            }
        }
    }
    let mut found = Vec::new();
    for &(user, path, text) in files {
        for (line, code) in text.lines().enumerate() {
            for word in identifiers(code) {
                if declared.get(word).is_some_and(|&owner| owner != user) {
                    found.push((path, line + 1, word));
                }
            }
        }
    }
    found
}

#[test]
fn no_crate_names_another_crates_own_world_trait() {
    let crates = crates();
    let own = "crates/test/rules/src/world.rs";
    let files: Vec<SourceFile> = sources()
        .into_iter()
        .filter(|file| file.path != own)
        .collect();
    let files: Vec<(&str, &str, &str)> = files
        .iter()
        .filter_map(|file| {
            let holder = holder(&crates, &file.path)?;
            Some((
                crates[holder].name.as_str(),
                file.path.as_str(),
                file.text.as_str(),
            ))
        })
        .collect();
    let found: Vec<String> = foreign_crate_traits(&files)
        .into_iter()
        .map(|(path, line, name)| format!("{path}:{line} {name}"))
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_crate_trait_rule_finds_each_use_outside_its_crate() {
    // `a` declares `AWorldCrate` and uses it; `b` imports it and names it in a bound; `c` names
    // `AWorldCrateInternals` and `AWorld`, other traits, and `XWorldCrate`, which no crate
    // declares.
    let files = [
        (
            "a",
            "a.rs",
            "pub(crate) trait AWorldCrate {}
impl AWorldCrate for World {}
",
        ),
        (
            "b",
            "b.rs",
            "use a::{AWorld, AWorldCrate};
fn f<W: AWorldCrate>() {}
",
        ),
        (
            "c",
            "c.rs",
            "use a::AWorldCrateInternals;
use a::AWorld;
use x::XWorldCrate;
",
        ),
    ];
    assert_eq!(
        foreign_crate_traits(&files),
        [("b.rs", 1, "AWorldCrate"), ("b.rs", 2, "AWorldCrate")]
    );
}

#[test]
fn the_world_rule_finds_each_function_that_takes_a_world_first() {
    // A free function and methods, of each form of a reference to a world, their visibility
    // read from their modifiers; a method of a trait's impl, one that takes the world later,
    // and one that takes a world by value are none.
    let text = "pub fn a(world: &World) {}\n\
        impl<'w> Owner<'w> {\n    pub(crate) const fn b<T>(\n        w: &'w mut World,\n    ) {}\n\
        \x20   pub(super) fn c(world: &mut World, x: u8) {}\n    fn d(x: u8, world: &World) {}\n}\n\
        impl Trait for Owner {\n    fn e(world: &World) {}\n}\n\
        fn f(world: World) {}\nfn g(world: &'a World) {}\n";
    let found = world_first(text);
    let expected = [
        (1, "a", true),
        (3, "Owner::b", true),
        (6, "Owner::c", false),
        (13, "g", false),
    ];
    let expected: Vec<WorldFirst> = expected
        .into_iter()
        .map(|(line, name, visible)| WorldFirst {
            line,
            name: name.to_owned(),
            visible,
        })
        .collect();
    assert_eq!(found, expected);
    // A name passed or listed as a value counts; a call, a path through it, a longer name and a
    // comment's mention do not.
    assert!(named_as_value(".add_systems(Owner::run)", "Owner::run"));
    assert!(named_as_value("[a, run, b]", "run"));
    assert!(!named_as_value("Owner::run(world)", "Owner::run"));
    assert!(!named_as_value("Owner::run::<T>(world)", "Owner::run"));
    assert!(!named_as_value(
        ".add_systems(Owner::running)",
        "Owner::run"
    ));
    assert!(!named_as_value("let x = run;", "run"));
    assert!(!named_as_value("/// As `Owner::run` does.", "Owner::run"));
}
