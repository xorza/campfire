use super::{NamedCode, capability_sources};

/// `code` with the body of each set's place left out, where a module names the steps of the
/// modules it builds on.
fn outside_places(code: &str) -> String {
    let mut out = String::new();
    let mut rest = code;
    while let Some(at) = rest.find("    fn place(self) -> SetPlace {") {
        out.push_str(&rest[..at]);
        let body = &rest[at..];
        let end = body.find("\n    }\n").expect("a place's body ends") + "\n    }\n".len();
        rest = &body[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn a_module_names_another_modules_sets_only_where_its_sets_run() {
    // A set orders against the steps of the modules it builds on, which its place names; a module
    // puts its systems in its own sets alone, so no system orders against another module's but
    // through its set's place (02-engine-core.md, Sets order against the steps of the modules
    // they build on).
    let module_of = |path: &str| path.split('/').next().unwrap_or_default().to_owned();
    let sources = capability_sources();
    let mut sets = Vec::new();
    for NamedCode { path, code } in &sources {
        for (at, _) in code.match_indices("impl ModuleSystems for ") {
            let rest = &code[at + "impl ModuleSystems for ".len()..];
            let end = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap();
            sets.push((module_of(path), rest[..end].to_owned()));
        }
    }
    assert!(
        sets.iter()
            .any(|(module, set)| module == "actions" && set == "ActionsSystems")
    );
    let mut named = Vec::new();
    let mut placed = Vec::new();
    for NamedCode { path, code } in &sources {
        let module = module_of(path);
        let outside = outside_places(code);
        if outside.len() < code.len() && code.contains("SetPlace::at(") {
            placed.push(module.clone());
        }
        // An import names the set for the place's sake; a statement of it may take lines.
        let mut importing = false;
        let used = outside.lines().filter(|line| {
            importing |= line.trim_start().starts_with("use ");
            let used = !importing;
            importing &= !line.trim_end().ends_with(';');
            used
        });
        let words =
            used.flat_map(|line| line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')));
        for word in words {
            // `NoSystems`, of a module that adds no system, is no module's.
            let other = sets
                .iter()
                .find(|(home, set)| set == word && *home != module && home != "declarations");
            if let Some((home, set)) = other {
                named.push(format!("{path} names {home}'s {set}"));
            }
        }
    }
    named.sort();
    named.dedup();
    assert_eq!(named, Vec::<String>::new());
    // The capabilities do place their sets at the steps of others: the scan sees them.
    assert!(placed.contains(&"combat".to_owned()), "{placed:?}");
    assert_eq!(
        outside_places("a\n    fn place(self) -> SetPlace {\n        X\n    }\nb"),
        "a\nb"
    );
}
