use std::cell::OnceCell;
use std::ops::Range;
use std::str::FromStr;

use proc_macro2::{LineColumn, Span, TokenStream, TokenTree};
use syn::visit::Visit;
use syn::{Attribute, Expr, ExprLit, Lit, Meta};

use crate::gated_items::GatedItems;

/// A gate of code that only tests and their helpers build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gate {
    /// `#[cfg(test)]`: a crate's own tests alone.
    Test,
    /// `#[cfg(feature = "internals")]`: the helpers of any crate's tests and benches.
    Internals,
}

impl Gate {
    /// The gate `attribute` writes, if it writes one alone.
    pub(crate) fn of(attribute: &Attribute) -> Option<Self> {
        Self::of_meta(&attribute.meta)
    }

    /// The gate an attribute whose content is `meta` writes, if it writes one alone.
    fn of_meta(meta: &Meta) -> Option<Self> {
        let Meta::List(list) = meta else {
            return None;
        };
        if !list.path.is_ident("cfg") {
            return None;
        }
        match list.parse_args::<Meta>().ok()? {
            Meta::Path(path) if path.is_ident("test") => Some(Self::Test),
            Meta::NameValue(pair) if pair.path.is_ident("feature") => match pair.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(name),
                    ..
                }) if name.value() == "internals" => Some(Self::Internals),
                _ => None,
            },
            _ => None,
        }
    }
}

/// A Rust source file a test read: its path from the directory it walked, its names joined by
/// `/` on every OS, and its text.
#[derive(Debug, Clone)]
pub(crate) struct SourceFile {
    pub(crate) path: String,
    pub(crate) text: String,
    /// The gate that holds its module, at its own declaration or an outer one's, so all of it is
    /// code only tests, or tests and their helpers, build; none for code every build holds.
    pub(crate) gate: Option<Gate>,
    /// Its text as Rust reads it, once a rule asks for it.
    analysis: OnceCell<Analysis>,
}

/// What a file's text is as Rust reads it: its code with each comment blanked, and the bytes of
/// each gated item with its gate.
#[derive(Debug, Clone)]
struct Analysis {
    code: String,
    gated: Vec<(Gate, Range<usize>)>,
}

impl SourceFile {
    /// The file at `path` that holds `text`, of no gated module.
    pub(crate) fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
            gate: None,
            analysis: OnceCell::new(),
        }
    }

    /// Its production code with no comment: its text with each item a gate holds and each
    /// comment blanked, line for line, so each line keeps its number; none for a gated module's
    /// file, a `tests.rs`, a `bench.rs` or a file of a `tests` directory, which hold tests and
    /// their helpers alone.
    pub(crate) fn production(&self) -> Option<String> {
        self.is_production()
            .then(|| self.blanked(&self.analysis().code, &[Gate::Test, Gate::Internals]))
    }

    /// Its production code, as `production` gives it, with its comments kept, as a doc link is
    /// a path the module names.
    pub(crate) fn production_with_comments(&self) -> Option<String> {
        self.is_production()
            .then(|| self.blanked(&self.text, &[Gate::Test, Gate::Internals]))
    }

    /// Its library code, what the crate's build with `internals` holds, with no comment: its text
    /// with each item `#[cfg(test)]` holds and each comment blanked; none for a module's file that
    /// gate holds, a `tests.rs` or a file of a `tests` directory. A `bench.rs` is library code, as
    /// `internals` builds it.
    pub(crate) fn library(&self) -> Option<String> {
        let (name, dirs) = self.names();
        let test = self.gate == Some(Gate::Test) || dirs.contains(&"tests") || name == "tests.rs";
        (!test).then(|| self.blanked(&self.analysis().code, &[Gate::Test]))
    }

    /// The paths of the modules a gate holds at their declarations at this file's top, each with
    /// its gate, by the path of the module's file and of its directory: `name.rs` or `name/mod.rs`
    /// beside it for a `lib.rs`, `main.rs` or `mod.rs`, else under its own name's directory. Its
    /// top tokens are read, not its items, as only a declaration names a module's file.
    pub(crate) fn gated_modules(&self) -> Vec<(String, Gate)> {
        if !self.text.contains("mod ") || !self.text.contains("#[cfg(") {
            return Vec::new();
        }
        let (dir, name) = self
            .path
            .rsplit_once('/')
            .unwrap_or(("", self.path.as_str()));
        let own = match name {
            "lib.rs" | "main.rs" | "mod.rs" => dir.to_owned(),
            _ => {
                let stem = name.strip_suffix(".rs").expect("a source is a `.rs` file");
                if dir.is_empty() {
                    stem.to_owned()
                } else {
                    format!("{dir}/{stem}")
                }
            }
        };
        let tokens = TokenStream::from_str(&self.text)
            .unwrap_or_else(|error| panic!("{}: {error}", self.path));
        let tokens: Vec<TokenTree> = tokens.into_iter().collect();
        let mut modules = Vec::new();
        let mut at = 0;
        while at < tokens.len() {
            let mut gate = None;
            while let [TokenTree::Punct(pound), TokenTree::Group(group), ..] = &tokens[at..]
                && pound.as_char() == '#'
            {
                let meta = syn::parse2::<Meta>(group.stream()).ok();
                gate = gate.or_else(|| meta.as_ref().and_then(Gate::of_meta));
                at += 2;
            }
            let declared = Self::declared_module(&tokens[at..]);
            if let (Some(gate), Some((module, len))) = (gate, declared) {
                let path = if own.is_empty() {
                    module
                } else {
                    format!("{own}/{module}")
                };
                modules.push((path, gate));
                at += len;
            } else {
                at += 1;
            }
        }
        modules
    }

    /// The module that `tokens` declare at their start, `mod name;` past its visibility, with the
    /// count of tokens the declaration takes.
    fn declared_module(tokens: &[TokenTree]) -> Option<(String, usize)> {
        let mut at = 0;
        if matches!(&tokens[at..], [TokenTree::Ident(word), ..] if word == "pub") {
            at += 1;
            if let [TokenTree::Group(_), ..] = &tokens[at..] {
                at += 1;
            }
        }
        match &tokens[at..] {
            [
                TokenTree::Ident(word),
                TokenTree::Ident(name),
                TokenTree::Punct(end),
                ..,
            ] if word == "mod" && end.as_char() == ';' => Some((name.to_string(), at + 3)),
            _ => None,
        }
    }

    /// Whether it may hold production code: no gated module's file, `tests.rs`, `bench.rs` or file
    /// of a `tests` directory.
    fn is_production(&self) -> bool {
        let (name, dirs) = self.names();
        self.gate.is_none() && !dirs.contains(&"tests") && name != "tests.rs" && name != "bench.rs"
    }

    /// Its file's name, and the directories its path holds it in.
    fn names(&self) -> (&str, Vec<&str>) {
        let mut names: Vec<&str> = self.path.split('/').collect();
        let name = names.pop().expect("a source has a name");
        (name, names)
    }

    fn analysis(&self) -> &Analysis {
        self.analysis
            .get_or_init(|| Analysis::of(&self.path, &self.text))
    }

    /// `view`, its text or its code, with each item one of `gates` holds blanked.
    fn blanked(&self, view: &str, gates: &[Gate]) -> String {
        if !self.text.contains("#[cfg(") {
            return view.to_owned();
        }
        let mut kept = view.as_bytes().to_vec();
        for (gate, bytes) in &self.analysis().gated {
            if gates.contains(gate) {
                blank(&mut kept[bytes.clone()]);
            }
        }
        String::from_utf8(kept).expect("a blank is a space")
    }
}

impl Analysis {
    /// `text`, of the file at `path`, lexed once, and parsed from its tokens where a gate may
    /// stand in it.
    fn of(path: &str, text: &str) -> Self {
        let tokens = TokenStream::from_str(text).unwrap_or_else(|error| panic!("{path}: {error}"));
        let lines = Lines::of(text);
        let mut code = text.as_bytes().to_vec();
        blank(&mut code);
        lines.keep(tokens.clone(), &mut code);
        let code = String::from_utf8(code).expect("a token is whole characters");
        if !text.contains("#[cfg(") {
            return Self {
                code,
                gated: Vec::new(),
            };
        }
        let file: syn::File = syn::parse2(tokens).unwrap_or_else(|error| panic!("{path}: {error}"));
        let mut items = GatedItems::default();
        items.visit_file(&file);
        let gated = (items.found().into_iter())
            .map(|(gate, span)| (gate, lines.bytes(span)))
            .collect();
        Self { code, gated }
    }
}

/// Each byte of `bytes` but a line's end as a space.
fn blank(bytes: &mut [u8]) {
    let mut start = 0;
    while let Some(end) = bytes[start..].iter().position(|&byte| byte == b'\n') {
        bytes[start..start + end].fill(b' ');
        start += end + 1;
    }
    bytes[start..].fill(b' ');
}

/// The byte where each line of a text starts, to place a span's line and column.
#[derive(Debug)]
struct Lines<'t> {
    text: &'t str,
    starts: Vec<usize>,
    /// Whether each line is ASCII, where a column is its byte.
    ascii: Vec<bool>,
}

impl<'t> Lines<'t> {
    fn of(text: &'t str) -> Self {
        let starts: Vec<usize> = [0]
            .into_iter()
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        let ascii = text.split('\n').map(str::is_ascii).collect();
        Self {
            text,
            starts,
            ascii,
        }
    }

    /// The byte at `at`, whose line counts from 1 and whose column counts characters from 0.
    fn byte(&self, at: LineColumn) -> usize {
        let line = at.line - 1;
        let start = self.starts[line];
        if self.ascii[line] {
            return start + at.column;
        }
        let end = self
            .starts
            .get(line + 1)
            .map_or(self.text.len(), |&next| next);
        let text = &self.text[start..end];
        start
            + text
                .char_indices()
                .nth(at.column)
                .map_or(text.len(), |(byte, _)| byte)
    }

    /// The bytes `span` covers.
    fn bytes(&self, span: Span) -> Range<usize> {
        self.byte(span.start())..self.byte(span.end())
    }

    /// Copies each token of `tokens` from the text into `kept`, a blanked copy of it, but a doc
    /// comment's, so a comment stays blank.
    fn keep(&self, tokens: TokenStream, kept: &mut [u8]) {
        for token in tokens {
            let opening = match &token {
                TokenTree::Group(group) => group.span_open(),
                _ => token.span(),
            };
            let at = &self.text[self.byte(opening.start())..];
            if at.starts_with("//") || at.starts_with("/*") {
                continue;
            }
            match token {
                TokenTree::Group(group) => {
                    self.copy(group.span_open(), kept);
                    self.copy(group.span_close(), kept);
                    self.keep(group.stream(), kept);
                }
                other => self.copy(other.span(), kept),
            }
        }
    }

    /// Copies the bytes `span` covers of the text into `kept`.
    fn copy(&self, span: Span, kept: &mut [u8]) {
        let bytes = self.bytes(span);
        kept[bytes.clone()].copy_from_slice(&self.text.as_bytes()[bytes]);
    }
}

/// `code`'s lines, each with no space at its end, so a blanked line reads as empty.
fn trimmed(code: &str) -> Vec<&str> {
    code.split('\n').map(str::trim_end).collect()
}

#[test]
fn a_test_file_holds_no_production_code_and_a_bench_is_library_code() {
    for path in ["src/tests.rs", "src/bench.rs", "src/a/tests/mod.rs"] {
        assert_eq!(
            SourceFile::new(path, "fn f() {}").production(),
            None,
            "{path}"
        );
    }
    assert_eq!(
        SourceFile::new("src/bench.rs", "fn f() {}")
            .library()
            .as_deref(),
        Some("fn f() {}")
    );
    assert_eq!(SourceFile::new("src/tests.rs", "fn f() {}").library(), None);
    assert_eq!(
        SourceFile::new("src/a/tests/mod.rs", "fn f() {}").library(),
        None
    );
    let gated = |gate| {
        let mut file = SourceFile::new("src/a.rs", "fn f() {}");
        file.gate = Some(gate);
        file
    };
    assert_eq!(gated(Gate::Internals).production(), None);
    assert_eq!(
        gated(Gate::Internals).library().as_deref(),
        Some("fn f() {}")
    );
    assert_eq!(gated(Gate::Test).library(), None);
}

#[test]
fn production_code_blanks_each_gated_item_and_keeps_each_line_at_its_number() {
    // A declaration of one line, a struct with its doc comment and attributes, a statement in a
    // function, an item whose attribute spans lines, a function whose signature spans lines, a
    // statement over several lines, an `if` with its `else`, a match arm, a field and a raw
    // string whose lines open at column 0, which the gate does not end.
    let text = concat!(
        "mod a;\n#[cfg(feature = \"internals\")]\nmod b;\nmod c;\n",
        "/// Its helpers.\n#[cfg(test)]\n#[derive(Debug)]\nstruct T {\n    x: u8,\n}\n",
        "fn f() {\n    let x = 1;\n    #[cfg(feature = \"internals\")]\n    if x == 1 {\n",
        "        g();\n    }\n    h();\n",
        "    #[cfg(test)]\n    world\n        .resource_mut::<R>()\n        .clear();\n    i();\n",
        "    #[cfg(test)]\n    if x {\n        j();\n    } else {\n        k();\n    }\n    l();\n}\n",
        "#[cfg(test)]\n#[cfg_attr(\n    debug_assertions,\n    expect(dead_code)\n)]\nfn u() {}\n",
        "#[cfg(test)]\nfn w<T>()\nwhere\n    T: Copy,\n{\n    m();\n}\nfn v() {}\n",
        "fn n(x: u8) -> u8 {\n    match x {\n        #[cfg(test)]\n        0 => 1,\n        _ => 2,\n    }\n}\n",
        "struct S {\n    #[cfg(test)]\n    y: u8,\n    z: u8,\n}\n",
        "#[cfg(test)]\nconst RAW: &str = r#\"\n}\nfn hidden() {}\n\"#;\nfn o() {}\n",
    );
    let code = SourceFile::new("src/a.rs", text).production().unwrap();
    let expected = concat!(
        "mod a;\n\n\nmod c;\n",
        "\n\n\n\n\n\n",
        "fn f() {\n    let x = 1;\n\n\n",
        "\n\n    h();\n",
        "\n\n\n\n    i();\n",
        "\n\n\n\n\n\n    l();\n}\n",
        "\n\n\n\n\n\n",
        "\n\n\n\n\n\n\nfn v() {}\n",
        "fn n(x: u8) -> u8 {\n    match x {\n\n\n        _ => 2,\n    }\n}\n",
        "struct S {\n\n         ,\n    z: u8,\n}\n",
        "\n\n\n\n\nfn o() {}\n",
    );
    assert_eq!(trimmed(&code), trimmed(expected));
    assert_eq!(code.lines().count(), text.lines().count());
    // The library keeps what `internals` holds, and blanks the rest alike.
    let library = SourceFile::new("src/a.rs", text).library().unwrap();
    let expected = concat!(
        "mod a;\n#[cfg(feature = \"internals\")]\nmod b;\nmod c;\n",
        "\n\n\n\n\n\n",
        "fn f() {\n    let x = 1;\n    #[cfg(feature = \"internals\")]\n    if x == 1 {\n",
        "        g();\n    }\n    h();\n",
        "\n\n\n\n    i();\n",
        "\n\n\n\n\n\n    l();\n}\n",
        "\n\n\n\n\n\n",
        "\n\n\n\n\n\n\nfn v() {}\n",
        "fn n(x: u8) -> u8 {\n    match x {\n\n\n        _ => 2,\n    }\n}\n",
        "struct S {\n\n         ,\n    z: u8,\n}\n",
        "\n\n\n\n\nfn o() {}\n",
    );
    assert_eq!(trimmed(&library), trimmed(expected));
}

#[test]
fn production_code_holds_no_comment_and_the_view_with_comments_keeps_them() {
    // A line comment, a block comment, a doc comment and a module's doc comment go, and a string
    // that holds `//` stays; each token keeps its column.
    let text = "//! A module.\n/// A doc naming b::f.\nfn f() {\n    g(); // a::h()\n    /* c::i */ let s = \"//\";\n}\n";
    let code = SourceFile::new("src/a.rs", text).production().unwrap();
    assert_eq!(
        trimmed(&code),
        [
            "",
            "",
            "fn f() {",
            "    g();",
            "               let s = \"//\";",
            "}",
            ""
        ]
    );
    assert_eq!(
        SourceFile::new("src/a.rs", text)
            .production_with_comments()
            .as_deref(),
        Some(text)
    );
}
