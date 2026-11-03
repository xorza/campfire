//! No genre word in production code: a mode names its heroes, lanes and resources in its data,
//! and the engine's words are the model's (`source/crates/capabilities/capabilities/AGENTS.md`).

use proc_macro2::{TokenStream, TokenTree};

use crate::allowlist::Allowlist;
use crate::capabilities::{NamedFile, capability_sources, in_capability};
use crate::parsed_code::ParsedCode;
use crate::workspace::Workspace;

/// The words no identifier of a capability's production code holds, each as a whole word; test
/// modules may hold them.
const GENRE_WORDS: [&str; 15] = [
    "hero", "heroes", "creep", "creeps", "lane", "lanes", "tower", "towers", "jungle", "mineral",
    "minerals", "gold", "mana", "barrack", "barracks",
];

/// The identifiers of `code`, as Rust's lexer reads them, so no comment, string or character
/// holds one.
fn identifiers(code: &ParsedCode<'_>) -> Vec<String> {
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
    collect(code.tokens().clone(), &mut found);
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

/// The genre words the identifiers of `production`, the production code of the file at `path`,
/// a capability crate's, hold; none of another file.
pub(crate) fn genre_words(path: &str, production: &ParsedCode<'_>) -> Vec<String> {
    if !in_capability(path) {
        return Vec::new();
    }
    identifiers(production)
        .iter()
        .flat_map(|identifier| words(identifier))
        .filter(|word| GENRE_WORDS.contains(&word.as_str()))
        .collect()
}

/// The games whose import keeps a module of the game's name, each as the words of that name; a
/// capability's code that names one keeps it in a module of that name (AGENTS.md, Game-bound code
/// lives in a module named after its game).
const GAMES: [[&str; 2]; 1] = [["zero", "hour"]];

/// The files outside a game's module that name a game, each for a reason: the importer's root,
/// which publishes its games' surface, its command line's list of games and the binary that
/// imports the one it names; the movement model's variant of a game's locomotor, the navigation
/// module's declaration of its parts and books, the locomotor data's section of it, and the walk
/// that drives its walkers.
const GAME_NAMED: [&str; 7] = [
    "crates/apps/import/src/lib.rs",
    "crates/apps/import/src/command_line/mod.rs",
    "crates/apps/import/src/main.rs",
    "crates/capabilities/navigation/src/movement_model.rs",
    "crates/capabilities/navigation/src/lib.rs",
    "crates/capabilities/navigation/src/locomotor_data/mod.rs",
    "crates/capabilities/navigation/src/walking.rs",
];

/// Whether the identifiers of `code` name a game of `GAMES`, its words in a row.
pub(crate) fn names_a_game(code: &ParsedCode<'_>) -> bool {
    // A code that holds no game's every word, in any case, names none, and its lexing is skipped.
    let lower = code.code.to_ascii_lowercase();
    if !GAMES
        .iter()
        .any(|game| game.iter().all(|word| lower.contains(word)))
    {
        return false;
    }
    identifiers(code).iter().any(|identifier| {
        let words = words(identifier);
        GAMES
            .iter()
            .any(|game| words.windows(game.len()).any(|run| run == game))
    })
}

/// Whether `path` lies in a module or a crate of a game's name, as `navigation/zero_hour/…` or
/// `checks/zero-hour-check/…`.
fn in_game_module(path: &str) -> bool {
    path.split('/').any(|segment| {
        let segment = segment.strip_suffix(".rs").unwrap_or(segment);
        let words: Vec<&str> = segment.split(['_', '-']).collect();
        GAMES
            .iter()
            .any(|game| words.windows(game.len()).any(|run| run == game))
    })
}

pub(crate) fn no_crate_names_a_game_outside_its_module(workspace: &Workspace) {
    let allowed = Allowlist::new(&GAME_NAMED);
    let mut named = Vec::new();
    for file in workspace.sources() {
        let path = file.path.as_str();
        if file.findings().names_a_game
            && !in_game_module(path)
            && !allowed.allows(|&listed| listed == path)
        {
            named.push(file.path.clone());
        }
    }
    assert_eq!(named, Vec::<String>::new());
    assert_eq!(allowed.unused(), Vec::<&&str>::new());
}

#[test]
fn a_game_is_named_by_its_words_in_a_row_and_its_module_holds_it() {
    let names_a_game = |code: &str| names_a_game(&ParsedCode::new("a.rs", code));
    assert!(names_a_game("struct ZeroHourDrive;") && names_a_game("mod zero_hour;"));
    assert!(!names_a_game("let zero = hour;") && !names_a_game("// ZeroHour"));
    assert!(in_game_module("navigation/zero_hour/mod.rs") && in_game_module("a/zero_hour.rs"));
    assert!(in_game_module("checks/zero-hour-check/src/lib.rs"));
    assert!(!in_game_module("navigation/walking.rs") && !in_game_module("a/zero/hour.rs"));
}

pub(crate) fn no_capability_names_a_genres_words(workspace: &Workspace) {
    let mut named = Vec::new();
    for NamedFile { path, file } in capability_sources(workspace) {
        for word in &file.findings().genre_words {
            named.push(format!("{path}: {word}"));
        }
    }
    named.sort();
    named.dedup();
    assert_eq!(named, Vec::<String>::new());
}

#[test]
fn a_genre_word_is_found_as_an_identifiers_whole_word() {
    // An identifier's whole word, in either case, is found; one inside another word, a comment,
    // a string, a raw string, a character and a lifetime are not.
    let code = concat!(
        "let tower_ai = TowerAi::new(goldfish, Hero);\n",
        "// a creep\n/* a lane */\n",
        "let s = \"mana\"; let r = r#\"gold\"#; let c = 'x';\n",
        "fn f<'a>(jungle: &'a str) {}\n",
    );
    assert_eq!(
        genre_words(
            "crates/capabilities/x/src/a.rs",
            &ParsedCode::new("a.rs", code)
        ),
        ["tower", "tower", "hero", "jungle"].map(str::to_owned)
    );
}
