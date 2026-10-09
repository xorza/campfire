use std::io;
use std::path::Path;

use campfire_store::{SourceFile, SourceFiles};

use super::*;

/// A step that failed for `source`.
#[derive(Debug)]
struct Step {
    name: &'static str,
    source: Option<io::Error>,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl Error for Step {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|error| error as &(dyn Error + 'static))
    }
}

#[test]
fn a_report_writes_each_error_down_to_the_root() {
    let alone = Step {
        name: "the key file",
        source: None,
    };
    assert_eq!(ErrorReport::of(&alone).to_string(), "the key file");

    // `io::Error::other` writes its inner error's message and gives that error's source, as a
    // transparent variant does, so the inner step shows once.
    let inner = Step {
        name: "not an nsec",
        source: Some(io::Error::new(io::ErrorKind::InvalidData, "bad checksum")),
    };
    let outer = Step {
        name: "the key file",
        source: Some(io::Error::other(inner)),
    };
    assert_eq!(
        ErrorReport::of(&outer).to_string(),
        "the key file: not an nsec: bad checksum"
    );
}

/// The lines that write an error's message alone and pass: Rhai's `EvalAltResult` has no source,
/// so its message is all of it; and clap writes its value parser's error into its own message,
/// which `Logging::command_line` logs as the usage error.
const ALONE: [&str; 2] = [
    "_ => ScriptError::Runtime(error.to_string()),",
    "error!(error = %error, \"the command line is refused\");",
];

/// The lines of `code` that write an error with its message alone, the workspace binding each
/// error it writes as `error`: as a log's `%` field, by `to_string`, or in a format string; and
/// the `#[error]` messages that write a field their error gives as its source.
fn unreported(code: &str) -> Vec<String> {
    let lines: Vec<&str> = code.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let alone = if trimmed.starts_with("#[error(") {
            writes_its_source(&lines[at..])
        } else {
            !ALONE.contains(&trimmed) && writes_alone(line)
        };
        if alone {
            found.push(trimmed.to_owned());
        }
    }
    found
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

/// Whether the `#[error]` attribute that starts `lines` writes a field of its variant that is the
/// error's source: the `#[source]` one, or one named `source`.
fn writes_its_source(lines: &[&str]) -> bool {
    let end = lines
        .iter()
        .position(|line| line.trim_end().ends_with(")]"))
        .unwrap();
    let message = lines[..=end].concat();
    let mut rest = lines[end + 1..]
        .iter()
        .map(|line| line.trim())
        .skip_while(|line| line.starts_with("///"));
    let Some(head) = rest.next() else {
        return false;
    };
    let mut sources = Vec::new();
    if head.contains("(#[source]") {
        sources.push("0");
    }
    if head.ends_with('{') {
        let mut source = false;
        for field in rest.take_while(|line| !line.starts_with('}')) {
            if field == "#[source]" {
                source = true;
                continue;
            }
            if let Some((name, _)) = field.split_once(':')
                && (source || name == "source")
            {
                sources.push(name);
            }
            source = false;
        }
    }
    sources.iter().any(|name| {
        [
            format!("{{{name}}}"),
            format!("{{{name}:"),
            format!(".{name}"),
        ]
        .iter()
        .any(|written| message.contains(written.as_str()))
    })
}

/// The production code of `file`, under `crates/` or `checks/`: none for one outside a member's
/// `src`.
fn production(file: &SourceFile) -> Option<&str> {
    let mut names = file.path.split('/');
    if names.nth(1) != Some("src") {
        return None;
    }
    file.production()
}

#[test]
fn every_error_the_workspace_writes_goes_through_its_report() {
    let workspace = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
    let mut found = Vec::new();
    let mut files = 0;
    for group in ["crates", "checks"] {
        for file in SourceFiles::under(&workspace.join(group)) {
            let Some(code) = production(&file) else {
                continue;
            };
            files += 1;
            for line in unreported(code) {
                found.push(format!("{group}/{}: {line}", file.path));
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // The walk reads the workspace, so a moved tree fails here, not silently.
    assert!(files > 100, "{files}");

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
            "#[error(\"the journal: {0}\")]\n    Journal(#[source] AppendError),",
            1,
        ),
        (
            "#[error(\"the journal\")]\n    Journal(#[source] AppendError),",
            0,
        ),
        (
            "#[error(\"no team {0}\")]\n    UnknownTeam(DeclaredName),",
            0,
        ),
        (
            "#[error(\"wall {wall}: {problem}\")]\n    Wall {\n        wall: usize,\n        #[source]\n        problem: PolygonError,\n    },",
            1,
        ),
        (
            "#[error(\"{}: {error}\", .path.display())]\n    Read {\n        path: PathBuf,\n        #[source]\n        error: io::Error,\n    },",
            1,
        ),
        (
            "#[error(\"{} does not read\", .path.display())]\n    Read {\n        path: PathBuf,\n        #[source]\n        error: io::Error,\n    },",
            0,
        ),
        ("LoadProblem::Mode(error) => write!(f, \"{error}\"),", 1),
        (
            "LoadProblem::Choice(problem) => write!(f, \"{problem}\"),",
            0,
        ),
    ];
    for (code, count) in samples {
        assert_eq!(unreported(code).len(), count, "{code}");
    }
}
