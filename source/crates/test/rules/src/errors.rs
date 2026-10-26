use proc_macro2::{TokenStream, TokenTree};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{Field, Lit, Meta, Variant};

use crate::allowlist::Allowlist;
use crate::source_file::SourceFile;
use crate::workspace::Workspace;

/// The lines that write an error's message alone and pass: Bevy's `BevyError`, a diff's, gives no
/// source, so the message is all of it; clap writes its value parser's error into its own message,
/// which `Logging::command_line` logs as the usage error; and a script failure's text writes each
/// error of its chain's message apart, in turn, which its text then writes as a report.
const ALONE: [&str; 3] = [
    "Self::ApplyDiff(error) => write!(f, \"a diff did not apply to its base: {error}\"),",
    "error!(error = %error, \"the command line is refused\");",
    "write!(self.text, \"{error}\").expect(\"text writes into a string\");",
];

/// The lines of `code`, the production code of the source `text`, that write an error with its
/// message alone, the workspace binding each error it writes as `error`: as a log's `%` field, by
/// `to_string`, or in a format string; and the `#[error]` messages of `text` that write a field
/// their error gives as its source, parsed from `text`, as a blanked test leaves no Rust.
fn unreported(code: &str, text: &str, alone: &Allowlist<&str>) -> Vec<String> {
    let lines: Vec<&str> = code.lines().collect();
    let mut found: Vec<String> = lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.starts_with("#[error("))
        .filter(|line| writes_alone(line) && !alone.allows(|listed| listed == line))
        .map(str::to_owned)
        .collect();
    if code.contains("#[error(") {
        let file = syn::parse_file(text).expect("a workspace source parses");
        let mut messages = SourceMessages { found: Vec::new() };
        messages.visit_file(&file);
        let production = messages
            .found
            .into_iter()
            .map(|line| lines[line - 1].trim());
        found.extend(
            production
                .filter(|line| !line.is_empty())
                .map(str::to_owned),
        );
    }
    found
}

/// The lines of the `#[error]` attributes that write a field of their variant that is the
/// error's source: the `#[source]` one, or one named `source`.
#[derive(Debug)]
struct SourceMessages {
    found: Vec<usize>,
}

impl Visit<'_> for SourceMessages {
    fn visit_variant(&mut self, variant: &Variant) {
        let Some(message) = (variant.attrs.iter()).find(|attr| attr.path().is_ident("error"))
        else {
            return;
        };
        let is_source = |field: &Field| {
            let marked = field
                .attrs
                .iter()
                .any(|attr| attr.path().is_ident("source"));
            marked || field.ident.as_ref().is_some_and(|name| name == "source")
        };
        let sources: Vec<String> = (variant.fields.iter().enumerate())
            .filter(|(_, field)| is_source(field))
            .map(|(at, field)| {
                field
                    .ident
                    .as_ref()
                    .map_or_else(|| at.to_string(), ToString::to_string)
            })
            .collect();
        let Meta::List(list) = &message.meta else {
            return;
        };
        if sources
            .iter()
            .any(|name| writes_field(list.tokens.clone(), name))
        {
            self.found.push(message.span().start().line);
        }
    }
}

/// Whether the arguments of an `#[error]` attribute, `tokens`, write the field `name`: in its
/// format string, as `{name}` or `{name:…}`, or as an argument's `.name`.
fn writes_field(tokens: TokenStream, name: &str) -> bool {
    let mut previous_dot = false;
    for token in tokens {
        match &token {
            TokenTree::Literal(literal) => {
                if let Lit::Str(text) = Lit::new(literal.clone()) {
                    let text = text.value();
                    if text.contains(&format!("{{{name}}}")) || text.contains(&format!("{{{name}:"))
                    {
                        return true;
                    }
                }
            }
            TokenTree::Ident(ident) if previous_dot && ident == name => return true,
            _ => {}
        }
        previous_dot = matches!(&token, TokenTree::Punct(punct) if punct.as_char() == '.');
    }
    false
}

/// Whether `line` writes the error bound to `error` with its message alone.
fn writes_alone(line: &str) -> bool {
    let field = line.match_indices('%').any(|(at, _)| {
        let path = path_at(&line[at + 1..]);
        path.rsplit('.').next() == Some("error") && !path.starts_with("self.")
    });
    let text = line.match_indices("error.to_string()").any(|(at, _)| {
        let before = &line[..at];
        !before.ends_with("self.") && !before.ends_with(|c: char| c.is_alphanumeric() || c == '_')
    });
    let format = ["{error}", "{error:"].iter().any(|written| {
        line.match_indices(written)
            .any(|(at, _)| !line[at + written.len()..].starts_with('?'))
    });
    field || text || format
}

/// The identifier path `rest` starts with, as `failure.error`.
fn path_at(rest: &str) -> &str {
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(rest.len());
    &rest[..end]
}

/// The production code of `file`, under `crates/` or `checks/`: none for one outside a member's
/// `src`.
fn production(file: &SourceFile) -> Option<&str> {
    if !file.path.split('/').any(|name| name == "src") {
        return None;
    }
    file.production()
}

pub(crate) fn every_error_the_workspace_writes_goes_through_its_report(workspace: &Workspace) {
    let alone = Allowlist::new(&ALONE);
    let mut found = Vec::new();
    let mut files = 0;
    for file in workspace.sources() {
        let Some(code) = production(file) else {
            continue;
        };
        files += 1;
        for line in unreported(code, &file.text, &alone) {
            found.push(format!("{}: {line}", file.path));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    assert_eq!(alone.unused(), Vec::<&&str>::new());
    // The walk reads the workspace, so a moved tree fails here, not silently.
    assert!(files > 100, "{files}");
}

#[test]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "the samples are lines of source the scan reads, not format strings"
)]
fn each_way_to_write_an_error_alone_is_found() {
    // Each way to write an error alone is found, and each way through its report passes.
    let samples = [
        ("error!(%error, \"the key does not read\");", 1),
        ("warn!(error = %failure.error, \"a package\");", 1),
        ("reason: error.to_string(),", 1),
        ("panic!(\"tick {tick}: {error}\");", 1),
        (
            "error!(error = %ErrorReport::of(&error), \"the key does not read\");",
            0,
        ),
        ("error: ErrorReport::of(&failure.error).to_string(),", 0),
        ("warn!(error = %self.error, \"{}\", Self::MESSAGE);", 0),
        ("panic!(\"tick {tick}: {error:?}\");", 0),
        (
            "enum E {\n    #[error(\"the journal: {0}\")]\n    Journal(#[source] AppendError),\n}",
            1,
        ),
        (
            "enum E {\n    #[error(\"the journal\")]\n    Journal(#[source] AppendError),\n}",
            0,
        ),
        (
            "enum E {\n    #[error(\"no team {0}\")]\n    UnknownTeam(DeclaredName),\n}",
            0,
        ),
        (
            "enum E {\n    #[error(\"wall {wall}: {problem}\")]\n    Wall {\n        wall: usize,\n        #[source]\n        problem: PolygonError,\n    },\n}",
            1,
        ),
        (
            "enum E {\n    #[error(\"{}: {error}\", .path.display())]\n    Read {\n        path: PathBuf,\n        #[source]\n        error: io::Error,\n    },\n}",
            1,
        ),
        (
            "enum E {\n    #[error(\"{} does not read\", .path.display())]\n    Read {\n        path: PathBuf,\n        #[source]\n        error: io::Error,\n    },\n}",
            0,
        ),
        ("LoadProblem::Mode(error) => write!(f, \"{error}\"),", 1),
        (
            "LoadProblem::Choice(problem) => write!(f, \"{problem}\"),",
            0,
        ),
    ];
    for (code, count) in samples {
        assert_eq!(
            unreported(code, code, &Allowlist::new(&ALONE)).len(),
            count,
            "{code}"
        );
    }
}
