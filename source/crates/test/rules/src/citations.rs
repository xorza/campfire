use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use campfire_store::{DirEntries, EntryKind, InputFile, PathError, ReadError};

use crate::source_file::SourceFile;
use crate::workspace::{Workspace, source};

/// The most bytes a test reads of a design document: far past any the design holds.
const DOC_LEN: usize = 1 << 20;

/// The workspace's files outside `crates/` and `checks/` whose comments cite the design.
const OTHER_FILES: [&str; 3] = ["Cargo.toml", "clippy.toml", ".config/nextest.toml"];

/// `text` with each comment's lines joined, so a citation a comment wraps reads whole.
fn joined(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut lines = text.lines();
    if let Some(first) = lines.next() {
        out.push_str(first);
    }
    for line in lines {
        let trimmed = line.trim_start();
        let rest = ["//!", "///", "//", "#"]
            .iter()
            .find_map(|lead| trimmed.strip_prefix(lead));
        if let Some(rest) = rest {
            out.push(' ');
            out.push_str(rest.trim_start());
        } else {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
}

/// A citation of a design's decision, by its document and title; the title empty for a whole
/// document.
#[derive(Debug)]
struct Citation {
    document: String,
    title: String,
}

/// Each citation in `text`, `(<document>.md, <title>)` or `(<document>.md)`, several in one
/// parenthesis apart by `; `.
fn citations(text: &str) -> Vec<Citation> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices('(') {
        // A Markdown link's target is a path, not a citation.
        if text[..at].ends_with(']') {
            continue;
        }
        let rest = &text[at + 1..];
        let Some(end) = rest.find(')') else {
            continue;
        };
        let inner = &rest[..end];
        let names_a_document = |part: &str| {
            let document = part.split(", ").next().unwrap_or_default();
            Path::new(document)
                .extension()
                .is_some_and(|extension| extension == "md")
                && document
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
        };
        if !names_a_document(inner) {
            continue;
        }
        for part in inner.split("; ") {
            let (document, title) = part.split_once(", ").unwrap_or((part, ""));
            found.push(Citation {
                document: document.to_owned(),
                title: title.to_owned(),
            });
        }
    }
    found
}

/// The titles a design document holds, and the bold leads of its bullets that never close.
#[derive(Debug)]
struct Titles {
    titles: Vec<String>,
    unclosed: Vec<String>,
}

/// The titles of `text`, a design document: each heading, and each bold lead of a bullet, nested
/// or not, with no stop or comma after it, read over the bullet's lines to its first blank line,
/// bullet or heading, so a lead that wraps reads whole.
fn titles(text: &str) -> Titles {
    let mut found = Titles {
        titles: Vec::new(),
        unclosed: Vec::new(),
    };
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some(heading) = line.strip_prefix('#') {
            found
                .titles
                .push(heading.trim_start_matches('#').trim().to_owned());
            continue;
        }
        let Some(lead) = line.trim_start().strip_prefix("- **") else {
            continue;
        };
        let mut bullet = lead.to_owned();
        while let Some(next) = lines.next_if(|next| {
            let next = next.trim_start();
            !next.is_empty() && !next.starts_with("- ") && !next.starts_with('#')
        }) {
            bullet.push(' ');
            bullet.push_str(next.trim_start());
        }
        match bullet.split_once("**") {
            Some((title, _)) => found
                .titles
                .push(title.trim_end_matches(['.', ',']).to_owned()),
            None => found.unclosed.push(bullet),
        }
    }
    found
}

/// Each citation of `files`, by the file that holds it, that names no document `documents` holds,
/// or a title its document lacks; and each mention of a design by its number, the old form.
fn broken(files: &[SourceFile], documents: &BTreeMap<String, Option<Titles>>) -> Vec<String> {
    let mut found = Vec::new();
    for (document, titles) in documents {
        for lead in titles.iter().flat_map(|titles| &titles.unclosed) {
            found.push(format!(
                "{document}: a bold lead that never closes: {lead:?}"
            ));
        }
    }
    for SourceFile {
        path: file, text, ..
    } in files
    {
        let text = joined(text);
        for Citation { document, title } in citations(&text) {
            match documents.get(&document) {
                Some(Some(titles)) if title.is_empty() || titles.titles.contains(&title) => {}
                Some(Some(_)) => found.push(format!("{file}: {document} holds no {title:?}")),
                _ => found.push(format!("{file}: no {document}")),
            }
        }
        if numbered_decisions(&text) {
            found.push(format!("{file}: a decision cited by its number"));
        }
        let lower = text.to_lowercase();
        for (at, _) in lower.match_indices("design ") {
            let digits = &lower.as_bytes()[at + 7..];
            if digits.len() >= 2 && digits[..2].iter().all(u8::is_ascii_digit) {
                found.push(format!("{file}: a design cited by its number"));
            }
        }
    }
    found
}

/// Whether `text` cites a decision by its number, `(D7)`, as a plan names its decisions, alone
/// or among the parts of a parenthesis.
fn numbered_decisions(text: &str) -> bool {
    text.match_indices('(').any(|(at, _)| {
        let rest = &text[at + 1..];
        let inner = rest.find(')').map_or("", |end| &rest[..end]);
        inner.split("; ").any(|part| {
            part.strip_prefix('D').is_some_and(|number| {
                !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
            })
        })
    })
}

/// The design's documents, each by its path under `docs/design/`, with its text, its wrapped lines
/// joined, so a citation a bullet wraps reads whole.
fn design_documents() -> Vec<SourceFile> {
    let mut found = Vec::new();
    let mut dirs = vec![String::new()];
    while let Some(dir) = dirs.pop() {
        let at = source().join("../docs/design").join(&dir);
        for entry in DirEntries::read(&at).unwrap() {
            let name = entry.name.to_str().expect("a document's name is UTF-8");
            let path = if dir.is_empty() {
                name.to_owned()
            } else {
                format!("{dir}/{name}")
            };
            match entry.kind {
                EntryKind::Dir => dirs.push(path),
                EntryKind::File if Path::new(name).extension().is_some_and(|ext| ext == "md") => {
                    let text = InputFile::read_text(&at.join(name), DOC_LEN).unwrap();
                    let text = text
                        .lines()
                        .map(str::trim_start)
                        .collect::<Vec<_>>()
                        .join(" ");
                    found.push(SourceFile::new(format!("docs/design/{path}"), text));
                }
                _ => {}
            }
        }
    }
    found.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    found
}

/// The document at `document` and its titles, a design document's path relative to `docs/design/`
/// or the repository's root, as `AGENTS.md`'s; none for a path that names no document.
fn document(document: &str) -> Option<Titles> {
    let root = source().join("..");
    let path = if document.starts_with("docs/design/") || document == "AGENTS.md" {
        root.join(document)
    } else {
        root.join("docs/design").join(document)
    };
    match InputFile::read_text(&path, DOC_LEN) {
        Ok(text) => Some(titles(&text)),
        Err(PathError {
            error: ReadError::Missing,
            ..
        }) => None,
        Err(error) => panic!("{error:?}"),
    }
}

pub(crate) fn a_citation_names_a_decision_its_document_holds(workspace: &Workspace) {
    // This file's own samples break the rule on purpose.
    let mut files: Vec<SourceFile> = workspace
        .sources()
        .iter()
        .filter(|file| file.path != "crates/test/rules/src/citations.rs")
        .cloned()
        .collect();
    for file in OTHER_FILES {
        files.push(SourceFile::new(
            file,
            InputFile::read_text(&source().join(file), DOC_LEN).unwrap(),
        ));
    }
    // The design's documents cite each other too; the walk reads them, so a moved tree fails.
    let design = design_documents();
    assert!(design.len() > 20, "{} documents", design.len());
    files.extend(design);
    let mut documents = BTreeMap::new();
    for file in &files {
        for Citation {
            document: cited, ..
        } in citations(&joined(&file.text))
        {
            documents
                .entry(cited.clone())
                .or_insert_with(|| document(&cited));
        }
    }
    assert_eq!(broken(&files, &documents), Vec::<String>::new());

    // A sample: a whole document, a title a wrapped comment holds, two in one parenthesis, a
    // title its bullet wraps, a nested bullet's, a missing title, a missing document, a link that
    // is no citation, and the old form; and a lead that never closes.
    let sample = "/// As (a.md) says, and (a.md, One title; a.md, Two\n/// title), not (a.md, Three) \
                  nor (b.md, One title); [a link](a.md#one-title); [a path](c.md); design 02, D1. \
                  (a.md, A long wrapped title) (a.md, Nested) (D7)\nfn f() {}\n";
    let documents = BTreeMap::from([
        (
            "a.md".to_owned(),
            Some(titles(
                "# A\n\n- **One title.** Text.\n- **Two title,** text.\n- **A long\n  wrapped \
                 title.** Text.\n  - **Nested.** Text.\n- **Never closed. Text.\n\n- **Four.** x\n",
            )),
        ),
        ("b.md".to_owned(), None),
    ]);
    assert_eq!(
        broken(&[SourceFile::new("s.rs", sample)], &documents),
        [
            "a.md: a bold lead that never closes: \"Never closed. Text.\"",
            "s.rs: a.md holds no \"Three\"",
            "s.rs: no b.md",
            "s.rs: a decision cited by its number",
            "s.rs: a design cited by its number",
        ]
    );
}

/// The relative paths `text`, a Markdown document's, links to: each `](path)` that names no
/// scheme, and not an anchor of its own document alone, with its anchor dropped.
fn relative_links(text: &str) -> Vec<&str> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("](") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find(')') else {
            break;
        };
        let target = &rest[..end];
        let path = target.split('#').next().unwrap_or_default();
        let outside = path.contains("://") || path.starts_with("mailto:");
        if !path.is_empty() && !target.contains(' ') && !outside {
            found.push(path);
        }
    }
    found
}

/// `path` with each `..` taking the component before it away, as a link's path resolves.
fn lexical(path: &Path) -> PathBuf {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            other => resolved.push(other),
        }
    }
    resolved
}

pub(crate) fn a_documents_link_names_a_file(_: &Workspace) {
    // The design's documents and `AGENTS.md`, each link resolved from its document's folder,
    // its target looked up in its folder's listing, each folder listed once.
    let root = source().join("..");
    let mut documents = design_documents();
    let agents = InputFile::read_text(&root.join("AGENTS.md"), DOC_LEN).unwrap();
    documents.push(SourceFile::new("AGENTS.md", agents));
    let mut listed: BTreeMap<PathBuf, BTreeSet<OsString>> = BTreeMap::new();
    let mut broken = Vec::new();
    for file in &documents {
        let folder = Path::new(&file.path)
            .parent()
            .expect("a document stands in a folder");
        for link in relative_links(&file.text) {
            let target = lexical(&folder.join(link));
            let (Some(parent), Some(name)) = (target.parent(), target.file_name()) else {
                broken.push(format!("{}: {link}", file.path));
                continue;
            };
            let names = listed.entry(parent.to_owned()).or_insert_with(|| {
                match DirEntries::read(&root.join(parent)) {
                    Ok(entries) => entries.into_iter().map(|entry| entry.name).collect(),
                    Err(PathError {
                        error: ReadError::Missing,
                        ..
                    }) => BTreeSet::new(),
                    Err(error) => panic!("{error:?}"),
                }
            });
            if !names.contains(name) {
                broken.push(format!("{}: {link}", file.path));
            }
        }
    }
    assert_eq!(broken, Vec::<String>::new());
    // A sample: a path, one up with an anchor, an anchor alone, a web link, and a bracket and
    // a parenthesis that hold prose, not a link.
    let sample = "[a](b.md) and [c](../d/e.md#x), [f](#g), [h](https://i.org/j), [k] (l), \
                  a[m](n o)";
    assert_eq!(relative_links(sample), ["b.md", "../d/e.md"]);
    assert_eq!(
        lexical(Path::new("docs/design/04-capabilities/../../research/x.md")),
        Path::new("docs/research/x.md")
    );
}
