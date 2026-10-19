//! No genre word in production code: a mode names its heroes, lanes and resources in its data,
//! and the engine's words are the model's (`source/crates/capabilities/capabilities/AGENTS.md`).

use super::*;

/// The words no identifier of a capability's production code holds, each as a whole word; test
/// modules may hold them.
const GENRE_WORDS: [&str; 15] = [
    "hero", "heroes", "creep", "creeps", "lane", "lanes", "tower", "towers", "jungle", "mineral",
    "minerals", "gold", "mana", "barrack", "barracks",
];

/// The files where a genre word has another meaning, each with the words it may hold there: the
/// grid's rows are tested four at a time in SIMD lanes.
const OTHER_MEANINGS: [(&str, &[&str]); 1] = [("geometry/grid/mod.rs", &["lane", "lanes"])];

/// The identifiers of `code`, with its comments, strings and characters left out.
fn identifiers(code: &str) -> Vec<&str> {
    let bytes = code.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &code[at..];
        if rest.starts_with("//") {
            at += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            at += rest.find("*/").map_or(rest.len(), |end| end + 2);
        } else if let Some(raw) = raw_string(rest) {
            at += raw;
        } else if bytes[at] == b'"' {
            at += quoted(rest, '"');
        } else if bytes[at] == b'\'' && quoted_char(rest) {
            at += quoted(rest, '\'');
        } else if bytes[at].is_ascii_alphabetic() || bytes[at] == b'_' {
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            found.push(&rest[..end]);
            at += end;
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    found
}

/// The length of the raw string `rest` opens with, `r"…"` or `r#"…"#`, if it opens with one.
fn raw_string(rest: &str) -> Option<usize> {
    let after = rest.strip_prefix('r')?;
    let hashes = after.len() - after.trim_start_matches('#').len();
    let body = after[hashes..].strip_prefix('"')?;
    let close = format!("\"{}", "#".repeat(hashes));
    let end = body.find(&close)?;
    Some(1 + hashes + 1 + end + close.len())
}

/// The length of the literal `rest` opens with, closed by `quote`, past its escapes.
fn quoted(rest: &str, quote: char) -> usize {
    let mut chars = rest.char_indices().skip(1);
    while let Some((at, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == quote {
            return at + 1;
        }
    }
    rest.len()
}

/// Whether the `'` that `rest` opens with starts a character, not a lifetime: one character or
/// an escape, then its closing `'`.
fn quoted_char(rest: &str) -> bool {
    let mut chars = rest.chars().skip(1);
    match chars.next() {
        Some('\\') => true,
        Some(_) => chars.next() == Some('\''),
        None => false,
    }
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
        .into_iter()
        .flat_map(words)
        .filter(|word| GENRE_WORDS.contains(&word.as_str()))
        .collect()
}

#[test]
fn no_capability_names_a_genres_words() {
    let mut named = Vec::new();
    capability_sources(&mut |path, code| {
        let other = OTHER_MEANINGS
            .iter()
            .find(|&&(file, _)| file == path)
            .map_or(&[][..], |&(_, words)| words);
        for word in genre_words(code) {
            if !other.contains(&word.as_str()) {
                named.push(format!("{path}: {word}"));
            }
        }
    });
    named.sort();
    named.dedup();
    assert_eq!(named, Vec::<String>::new());

    // An identifier's whole word, in either case, is found; one inside another word, a comment,
    // a string, a raw string and a character are not; a lifetime does not open a character.
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
