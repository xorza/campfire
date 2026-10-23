use std::collections::{BTreeMap, BTreeSet};

use crate::source_file::SourceFile;
use crate::workspace::Workspace;

/// The cycles of `imports`, a crate's top modules each with the top modules it imports: each
/// group of two or more modules that reach each other through their imports, sorted.
fn module_cycles(imports: &BTreeMap<String, BTreeSet<String>>) -> Vec<Vec<String>> {
    let reach = |from: &str| {
        let mut seen = BTreeSet::new();
        let mut next = vec![from.to_owned()];
        while let Some(module) = next.pop() {
            for other in imports.get(&module).into_iter().flatten() {
                if seen.insert(other.clone()) {
                    next.push(other.clone());
                }
            }
        }
        seen
    };
    let reached: BTreeMap<&String, BTreeSet<String>> = imports
        .keys()
        .map(|module| (module, reach(module)))
        .collect();
    let mut cycles: Vec<Vec<String>> = Vec::new();
    for (module, from) in &reached {
        let cycle: Vec<String> = from
            .iter()
            .filter(|other| {
                reached
                    .get(other)
                    .is_some_and(|back| back.contains(*module))
            })
            .cloned()
            .collect();
        if cycle.len() > 1 && !cycles.contains(&cycle) {
            cycles.push(cycle);
        }
    }
    cycles
}

/// Each path `code` names through `crate::`, by its segments, each path of a group its own:
/// `use crate::a::{b::X, c};` names `a::b::X` and `a::c`, and `crate::d::f()` names `d::f`. A
/// group's `self` names the path before it, and a glob that path. A macro's `$crate::` names no
/// module of the crate it is written in.
fn crate_paths(code: &str) -> Vec<Vec<&str>> {
    let mut paths = Vec::new();
    for (at, _) in code.match_indices("crate::") {
        if code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$') {
            continue;
        }
        use_tree(&code[at + "crate::".len()..], Vec::new(), &mut paths);
    }
    paths.retain(|path| !path.is_empty());
    paths
}

/// Reads the use tree at the start of `text`, under the segments `path`, and pushes each path
/// it names to `paths`; how much of `text` it read.
fn use_tree<'a>(text: &'a str, mut path: Vec<&'a str>, paths: &mut Vec<Vec<&'a str>>) -> usize {
    let skip = |at: usize| at + (text[at..].len() - text[at..].trim_start().len());
    let mut at = skip(0);
    loop {
        if text[at..].starts_with('{') {
            at = skip(at + 1);
            while !text[at..].starts_with('}') && at < text.len() {
                at = skip(at + use_tree(&text[at..], path.clone(), paths));
                if text[at..].starts_with(',') {
                    at = skip(at + 1);
                } else if !text[at..].starts_with('}') {
                    return at;
                }
            }
            return (at + 1).min(text.len());
        }
        let rest = &text[at..];
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        match &rest[..end] {
            "" | "self" => {}
            segment => path.push(segment),
        }
        at += end;
        if text[at..].starts_with("::") {
            at += "::".len();
        } else {
            paths.push(path);
            return at;
        }
    }
}

/// Each module of the crate at `src/` that has children, by its path, its children each with
/// the siblings its production code names: a child of `a::b` names its sibling `c` by a path
/// through `crate::a::b::c`. The crate's root is the module of no path.
fn sibling_imports(
    src: &str,
    sources: &[SourceFile],
) -> BTreeMap<Vec<String>, BTreeMap<String, BTreeSet<String>>> {
    let mut files = Vec::new();
    for file in sources {
        let Some(rest) = file.path.strip_prefix(&format!("{src}/")) else {
            continue;
        };
        let mut module: Vec<String> = rest
            .trim_end_matches(".rs")
            .split('/')
            .map(str::to_owned)
            .collect();
        if matches!(
            module.last().map(String::as_str),
            Some("mod" | "lib" | "main")
        ) {
            module.pop();
        }
        files.push((module, file));
    }
    let mut parents: BTreeMap<Vec<String>, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    for (module, _) in &files {
        for depth in 0..module.len() {
            let children = parents.entry(module[..depth].to_vec()).or_default();
            children.entry(module[depth].clone()).or_default();
        }
    }
    for (module, file) in &files {
        let Some(code) = file.production_with_comments() else {
            continue;
        };
        let paths = crate_paths(&code);
        for depth in 0..module.len() {
            let (parent, child) = (&module[..depth], &module[depth]);
            let children = parents.get_mut(parent).expect("each parent is listed");
            for path in &paths {
                let through = path.len() > depth && path.iter().zip(parent).all(|(a, b)| a == b);
                let Some(&sibling) = path.get(depth).filter(|_| through) else {
                    continue;
                };
                if sibling != child && children.contains_key(sibling) {
                    let named = children.get_mut(child).expect("each child is listed");
                    named.insert(sibling.to_owned());
                }
            }
        }
    }
    parents
}

#[test]
fn no_crates_modules_import_each_other_in_a_cycle() {
    let workspace = Workspace::read();
    let sources = workspace.sources();
    let mut found = Vec::new();
    for krate in workspace.crates() {
        let src = format!("{}/src", krate.path);
        for (parent, imports) in sibling_imports(&src, sources) {
            let at: String = parent
                .iter()
                .flat_map(|module| ["::", module.as_str()])
                .collect();
            found.extend(
                module_cycles(&imports)
                    .into_iter()
                    .map(|cycle| format!("{}{at}: {}", krate.name, cycle.join(", "))),
            );
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // Three modules that import in a ring make one cycle; a module that only imports into it stays
    // out, as does a chain.
    let imports: BTreeMap<String, BTreeSet<String>> = [
        ("a", &["b"][..]),
        ("b", &["c"]),
        ("c", &["a"]),
        ("d", &["a"]),
        ("e", &["f"]),
        ("f", &[]),
    ]
    .into_iter()
    .map(|(module, named)| {
        (
            module.to_owned(),
            named.iter().map(|&name| name.to_owned()).collect(),
        )
    })
    .collect();
    assert_eq!(module_cycles(&imports), [["a", "b", "c"]]);
    // A path, a module, a group with a nested group, `self` and a trailing comma, a glob, a call
    // and a macro's own crate.
    let code = "use crate::a::X;\nuse crate::b;\nuse crate::{c::{self, Y, z::Z}, d,\n};\nuse crate::e::*;\ncrate::f::g();\n$crate::h::H";
    let paths: Vec<String> = crate_paths(code)
        .iter()
        .map(|path| path.join("::"))
        .collect();
    assert_eq!(
        paths,
        ["a::X", "b", "c", "c::Y", "c::z::Z", "d", "e", "f::g"]
    );
    // Under the crate's `p`, its child `q` names `r` through a group, and `r` names `q` through a
    // path: a cycle of `p`'s children. The root's `p` naming itself is none.
    let file = |path: &str, text: &str| SourceFile::new(format!("src/{path}"), text);
    let files = [
        file("lib.rs", "mod p;"),
        file("p/mod.rs", "use crate::p::q::Q;"),
        file("p/q.rs", "use crate::p::{r::R, s};"),
        file("p/r.rs", "use crate::p::q::Q;"),
        file("p/s/mod.rs", ""),
    ];
    let imports = sibling_imports("src", &files);
    let under_p = &imports[&vec!["p".to_owned()]];
    assert_eq!(module_cycles(under_p), [["q", "r"]]);
    assert!(module_cycles(&imports[&Vec::new()]).is_empty());
}
