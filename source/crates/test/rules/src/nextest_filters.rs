//! Each filter of the nextest configuration names tests the workspace holds, so a rename that
//! leaves an override matching nothing fails here, not as a test that loses its bound.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use proc_macro2::{TokenStream, TokenTree};
use serde::Deserialize;

use campfire_codec::Toml;
use campfire_store::InputFile;

use crate::workspace::{Workspace, source};

/// The most bytes the nextest configuration holds: far past the workspace's.
const NEXTEST_LEN: usize = 1 << 16;

/// The nextest configuration, its overrides' filters alone.
#[derive(Debug, Deserialize)]
struct NextestConfig {
    profile: BTreeMap<String, NextestProfile>,
}

#[derive(Debug, Deserialize)]
struct NextestProfile {
    #[serde(default)]
    overrides: Vec<NextestOverride>,
}

#[derive(Debug, Deserialize)]
struct NextestOverride {
    filter: String,
}

/// A term of a filter, of the forms the configuration writes, joined by `&`.
#[derive(Debug, PartialEq, Eq)]
enum FilterTerm<'a> {
    Package(&'a str),
    /// A test's path, exact, a regular expression or a part of it: the words it names.
    Test(Vec<&'a str>),
}

impl<'a> FilterTerm<'a> {
    /// The terms of `filter`; a panic on a form the rule does not read, so a new one is read
    /// before it passes.
    fn of(filter: &'a str) -> Vec<Self> {
        filter
            .split('&')
            .map(|term| {
                let term = term.trim();
                if let Some(name) = term
                    .strip_prefix("package(")
                    .and_then(|t| t.strip_suffix(')'))
                {
                    Self::Package(name)
                } else if let Some(pattern) =
                    term.strip_prefix("test(").and_then(|t| t.strip_suffix(')'))
                {
                    let words = pattern.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
                    Self::Test(words.filter(|word| !word.is_empty()).collect())
                } else {
                    panic!("a filter's term the rule reads: {term:?}")
                }
            })
            .collect()
    }
}

/// The names of `tokens` a test's path may hold: each module's and each function's.
fn path_names(tokens: TokenStream, names: &mut BTreeSet<String>) {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    for (at, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Ident(keyword) if keyword == "fn" || keyword == "mod" => {
                if let Some(TokenTree::Ident(name)) = tokens.get(at + 1) {
                    names.insert(name.to_string());
                }
            }
            TokenTree::Group(group) => path_names(group.stream(), names),
            _ => {}
        }
    }
}

/// The words of `filter` no name of the packages it names holds, each package's names by
/// `names`; every package's for a filter that names none.
fn unmatched<'a>(filter: &'a str, names: &BTreeMap<String, BTreeSet<String>>) -> Vec<&'a str> {
    let terms = FilterTerm::of(filter);
    let packages: Vec<&str> = (terms.iter())
        .filter_map(|term| match term {
            FilterTerm::Package(name) => Some(*name),
            FilterTerm::Test(_) => None,
        })
        .collect();
    let mut found: Vec<&str> = (packages.iter().copied())
        .filter(|package| !names.contains_key(*package))
        .collect();
    let searched: Vec<&BTreeSet<String>> = if packages.is_empty() {
        names.values().collect()
    } else {
        packages
            .iter()
            .filter_map(|package| names.get(*package))
            .collect()
    };
    for term in &terms {
        if let FilterTerm::Test(words) = term {
            found.extend(words.iter().filter(|word| {
                !searched
                    .iter()
                    .any(|names| names.iter().any(|name| name.contains(**word)))
            }));
        }
    }
    found
}

pub(crate) fn each_nextest_filter_names_a_test(workspace: &Workspace) {
    let text = InputFile::read_text(&source().join(".config/nextest.toml"), NEXTEST_LEN).unwrap();
    let config: NextestConfig = Toml::parse(&text).expect("the nextest configuration reads");
    let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for krate in workspace.crates() {
        let found = names.entry(krate.name.clone()).or_default();
        for file in workspace.sources_of(krate) {
            let stem = file.path.rsplit('/').next().unwrap_or_default();
            found.insert(stem.trim_end_matches(".rs").to_owned());
            let tokens = TokenStream::from_str(file.code()).expect("a workspace source lexes");
            path_names(tokens, found);
        }
    }
    let filters: Vec<&str> = (config.profile.values())
        .flat_map(|profile| &profile.overrides)
        .map(|over| over.filter.as_str())
        .collect();
    assert!(filters.len() > 5, "{filters:?}");
    let found: Vec<String> = filters
        .iter()
        .flat_map(|filter| {
            unmatched(filter, &names)
                .into_iter()
                .map(move |word| format!("{filter}: {word}"))
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_filter_word_no_name_holds_is_found() {
    // An exact path, a regular expression with a group and a part each find their names; a
    // renamed test, a missing module and an unknown package are found.
    let names = BTreeMap::from([(
        "campfire-a".to_owned(),
        BTreeSet::from(["runner", "scripted_match", "every_flaw_of_types"].map(str::to_owned)),
    )]);
    let found = |filter| unmatched(filter, &names);
    assert!(found("package(campfire-a) & test(=runner::scripted_match)").is_empty());
    assert!(found("test(/(^|::)scripted_match::/)").is_empty());
    assert!(found("package(campfire-a) & test(/every_flaw_of_/)").is_empty());
    assert_eq!(
        found("package(campfire-a) & test(/^runner::(scripted_match|resume)/)"),
        ["resume"]
    );
    assert_eq!(
        found("package(campfire-b) & test(/x/)"),
        ["campfire-b", "x"]
    );
    assert_eq!(
        FilterTerm::of("package(p) & test(=a::b)"),
        [FilterTerm::Package("p"), FilterTerm::Test(vec!["a", "b"])]
    );
}
