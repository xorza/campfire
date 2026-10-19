use std::collections::BTreeMap;
use std::path::Path;

use campfire_store::{InputFile, PathError, ReadError};

use crate::source_file::SourceFile;
use crate::workspace::{source, sources};

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

/// The titles `text`, a design document, holds: each heading, and each bold lead of a bullet,
/// with no stop or comma after it.
fn titles(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in text.lines() {
        if let Some(heading) = line.strip_prefix('#') {
            found.push(heading.trim_start_matches('#').trim().to_owned());
        } else if let Some(lead) = line.strip_prefix("- **")
            && let Some((title, _)) = lead.split_once("**")
        {
            found.push(title.trim_end_matches(['.', ',']).to_owned());
        }
    }
    found
}

/// Each citation of `files`, by the file that holds it, that names no document `documents` holds,
/// or a title its document lacks; and each mention of a design by its number, the old form.
fn broken(files: &[SourceFile], documents: &BTreeMap<String, Option<Vec<String>>>) -> Vec<String> {
    let mut found = Vec::new();
    for SourceFile {
        path: file, text, ..
    } in files
    {
        let text = joined(text);
        for Citation { document, title } in citations(&text) {
            match documents.get(&document) {
                Some(Some(titles)) if title.is_empty() || titles.contains(&title) => {}
                Some(Some(_)) => found.push(format!("{file}: {document} holds no {title:?}")),
                _ => found.push(format!("{file}: no {document}")),
            }
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

/// The document at `document` and its titles, a design document's path relative to `docs/design/`
/// or the repository's root, as `AGENTS.md`'s; none for a path that names no document.
fn document(document: &str) -> Option<Vec<String>> {
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

#[test]
fn a_citation_names_a_decision_its_document_holds() {
    // This file's own samples break the rule on purpose.
    let mut files: Vec<SourceFile> = sources()
        .into_iter()
        .filter(|file| file.path != "crates/test/rules/src/citations.rs")
        .collect();
    for file in OTHER_FILES {
        files.push(SourceFile::new(
            file,
            InputFile::read_text(&source().join(file), DOC_LEN).unwrap(),
        ));
    }
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
    // missing title, a missing document, a link that is no citation, and the old form.
    let sample = "/// As (a.md) says, and (a.md, One title; a.md, Two\n/// title), not (a.md, Three) \
                  nor (b.md, One title); [a link](a.md#one-title); design 02, D1.\nfn f() {}\n";
    let documents = BTreeMap::from([
        (
            "a.md".to_owned(),
            Some(titles(
                "# A\n\n- **One title.** Text.\n- **Two title,** text.\n",
            )),
        ),
        ("b.md".to_owned(), None),
    ]);
    assert_eq!(
        broken(&[SourceFile::new("s.rs", sample)], &documents),
        [
            "s.rs: a.md holds no \"Three\"",
            "s.rs: no b.md",
            "s.rs: a design cited by its number",
        ]
    );
}
