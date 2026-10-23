//! No genre word in production code: a mode names its heroes, lanes and resources in its data,
//! and the engine's words are the model's (`source/crates/capabilities/capabilities/AGENTS.md`).

use std::str::FromStr;

use proc_macro2::{TokenStream, TokenTree};

use super::{NamedCode, capability_sources};

/// The words no identifier of a capability's production code holds, each as a whole word; test
/// modules may hold them.
const GENRE_WORDS: [&str; 15] = [
    "hero", "heroes", "creep", "creeps", "lane", "lanes", "tower", "towers", "jungle", "mineral",
    "minerals", "gold", "mana", "barrack", "barracks",
];

/// The identifiers of `code`, as Rust's lexer reads them, so no comment, string or character
/// holds one.
fn identifiers(code: &str) -> Vec<String> {
    fn collect(tokens: TokenStream, found: &mut Vec<String>) {
        for token in tokens {
            match token {
                TokenTree::Group(group) => collect(group.stream(), found),
                TokenTree::Ident(ident) => found.push(ident.to_string()),
                TokenTree::Punct(_) | TokenTree::Literal(_) => {}
            }
        }
    }
    let mut found = Vec::new();
    collect(
        TokenStream::from_str(code).expect("a source of the workspace lexes"),
        &mut found,
    );
    found
}

/// The words of `identifier`, lowercase: its parts between underscores, each split before each
/// capital, as `TowerAi` and `tower_ai` both hold `tower`.
fn words(identifier: &str) -> Vec<String> {
    let mut found = Vec::new();
    for part in identifier.split('_') {
        let mut word = String::new();
        for c in part.chars() {
            if c.is_ascii_uppercase() && !word.is_empty() {
                found.push(word.to_ascii_lowercase());
                word.clear();
            }
            word.push(c);
        }
        if !word.is_empty() {
            found.push(word.to_ascii_lowercase());
        }
    }
    found
}

/// The genre words the identifiers of `code` hold.
fn genre_words(code: &str) -> Vec<String> {
    identifiers(code)
        .iter()
        .flat_map(|identifier| words(identifier))
        .filter(|word| GENRE_WORDS.contains(&word.as_str()))
        .collect()
}

#[test]
fn no_capability_names_a_genres_words() {
    let mut named = Vec::new();
    for NamedCode { path, code } in capability_sources() {
        for word in genre_words(&code) {
            named.push(format!("{path}: {word}"));
        }
    }
    named.sort();
    named.dedup();
    assert_eq!(named, Vec::<String>::new());

    // An identifier's whole word, in either case, is found; one inside another word, a comment,
    // a string, a raw string, a character and a lifetime are not.
    let code = concat!(
        "let tower_ai = TowerAi::new(goldfish, Hero);\n",
        "// a creep\n/* a lane */\n",
        "let s = \"mana\"; let r = r#\"gold\"#; let c = 'x';\n",
        "fn f<'a>(jungle: &'a str) {}\n",
    );
    assert_eq!(
        genre_words(code),
        ["tower", "tower", "hero", "jungle"].map(str::to_owned)
    );
}
