use std::str::FromStr;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

/// A path the code of a source names: its segments joined by `::`, each group of a `use` given
/// whole, so `use std::{fs, io}` names `std::fs` and `std::io`; with the line of its first
/// segment, and the line of the `cfg`, `cfg_attr` or `cfg!` whose parenthesis holds it. A comment
/// names none, nor a string.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NamedPath {
    pub(crate) text: String,
    pub(crate) line: usize,
    pub(crate) cfg: Option<usize>,
}

impl NamedPath {
    /// The paths `code` names, in its order.
    pub(crate) fn of(code: &str) -> Vec<Self> {
        let tokens = TokenStream::from_str(code).expect("a workspace source lexes");
        let mut found = Vec::new();
        walk(tokens, "", None, &mut found);
        found
    }

    /// Whether any of `paths` holds `name`, a path or a part of one, as `std::fs` or `HashMap`.
    pub(crate) fn holds(paths: &[Self], name: &str) -> bool {
        paths.iter().any(|path| path.text.contains(name))
    }
}

/// Takes the paths of `tokens`, each after `prefix`, the path a `use` group stands under, into
/// `found`; `cfg` the line of the `cfg` whose condition `tokens` are.
fn walk(tokens: TokenStream, prefix: &str, cfg: Option<usize>, found: &mut Vec<NamedPath>) {
    let tokens: Vec<TokenTree> = tokens.into_iter().collect();
    let mut at = 0;
    while at < tokens.len() {
        match &tokens[at] {
            TokenTree::Ident(first) => {
                let line = first.span().start().line;
                let mut text = format!("{prefix}{first}");
                at += 1;
                let mut grouped = false;
                while separator(&tokens[at..]) {
                    match tokens.get(at + 2) {
                        Some(TokenTree::Ident(next)) => {
                            text.push_str("::");
                            text.push_str(&next.to_string());
                            at += 3;
                        }
                        Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Brace => {
                            walk(group.stream(), &format!("{text}::"), cfg, found);
                            at += 3;
                            grouped = true;
                            break;
                        }
                        _ => break,
                    }
                }
                if grouped {
                    continue;
                }
                let condition = (text == "cfg" || text == "cfg_attr").then_some(line);
                if let Some(cfg_line) = condition {
                    let bang =
                        matches!(&tokens.get(at), Some(TokenTree::Punct(p)) if p.as_char() == '!');
                    let skip = usize::from(bang);
                    if let Some(TokenTree::Group(group)) = tokens.get(at + skip)
                        && group.delimiter() == Delimiter::Parenthesis
                    {
                        walk(group.stream(), "", Some(cfg_line), found);
                        at += skip + 1;
                    }
                }
                found.push(NamedPath { text, line, cfg });
            }
            TokenTree::Group(group) => {
                walk(group.stream(), "", cfg, found);
                at += 1;
            }
            TokenTree::Punct(_) | TokenTree::Literal(_) => at += 1,
        }
    }
}

/// Whether `tokens` start with `::`.
fn separator(tokens: &[TokenTree]) -> bool {
    matches!(
        tokens,
        [TokenTree::Punct(first), TokenTree::Punct(second), ..]
            if first.as_char() == ':' && first.spacing() == Spacing::Joint && second.as_char() == ':'
    )
}

#[test]
fn a_path_reads_whole_through_groups_and_never_from_a_comment() {
    let text = |code: &str| -> Vec<String> {
        NamedPath::of(code)
            .into_iter()
            .map(|path| path.text)
            .collect()
    };
    // A wrapped group gives each of its paths whole, a nested one too, and `self` its own.
    assert_eq!(
        text("use std::{\n    fs,\n    io::{self, Read},\n};"),
        ["use", "std::fs", "std::io::self", "std::io::Read"]
    );
    // A path in an expression, a method's name, a generic's argument; a doc, a comment and a
    // string name nothing but the doc's attribute.
    assert_eq!(
        text(
            "/// Uses `std::fs`.\nfn f() { // std::fs\n    x.par_iter(\"std::fs\"); Vec::<HashMap>::new() }"
        ),
        ["doc", "fn", "f", "x", "par_iter", "Vec", "HashMap", "new"]
    );
    // A condition's paths carry its line; a `cfg!` too; others none.
    let paths = NamedPath::of("#[cfg(all(\n    unix,\n    test))]\nfn f() { cfg!(windows) }");
    let conditions: Vec<(&str, usize, Option<usize>)> = (paths.iter())
        .map(|path| (path.text.as_str(), path.line, path.cfg))
        .collect();
    assert_eq!(
        conditions,
        [
            ("all", 1, Some(1)),
            ("unix", 2, Some(1)),
            ("test", 3, Some(1)),
            ("cfg", 1, None),
            ("fn", 4, None),
            ("f", 4, None),
            ("windows", 4, Some(4)),
            ("cfg", 4, None),
        ]
    );
    assert!(NamedPath::holds(&paths, "unix"));
    assert!(!NamedPath::holds(&paths, "macos"));
}
