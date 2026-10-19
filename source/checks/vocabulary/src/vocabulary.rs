use std::ffi::OsStr;
use std::fmt::{self, Write};
use std::path::Path;

use campfire_common::ErrorReport;

use crate::error::VocabularyError;
use crate::index::Index;
use crate::outline::{self, Outline};
use crate::repo_file::{FileKind, RepoFile};

/// What the text says it holds, at its head.
const HEADER: &str = "\
The vocabulary of the repository, written by `cargo drive run vocabulary`.

Layout: each file git tracks or would track, ignored ones left out, a nested repository as one
line; under each Rust file its items, and under each item what it holds, without `use` items,
`mod` declarations, items in function bodies, and `where` clauses.
Index: each name the Rust files declare, by section and by name, case aside, then what declares
it and where; a test function is in the layout alone.
Words: each lowercase word of the index's names and of the layout's directories and files, with
how many names hold it.
";

/// The repository's layout, each Rust file's items in it, and the index of their names.
#[derive(Debug)]
pub(crate) struct Vocabulary {
    layout: String,
    index: Index,
    dirs: usize,
    files: usize,
    rust_files: usize,
    unparsed: Vec<Unparsed>,
}

/// A Rust file that does not parse, which the layout marks.
#[derive(Debug)]
pub(crate) struct Unparsed {
    pub(crate) path: String,
    pub(crate) error: syn::Error,
}

impl Vocabulary {
    /// The vocabulary of `files`, which go by their paths' names in order, each Rust file's text
    /// given by `read` from its path.
    pub(crate) fn of(
        files: &[RepoFile],
        mut read: impl FnMut(&str) -> Result<String, VocabularyError>,
    ) -> Result<Self, VocabularyError> {
        let mut vocabulary = Self {
            layout: String::new(),
            index: Index::default(),
            dirs: 0,
            files: 0,
            rust_files: 0,
            unparsed: Vec::new(),
        };
        debug_assert!(
            files.is_sorted_by(|a, b| a.path.split('/').lt(b.path.split('/'))),
            "each directory's files come together, and each file once"
        );
        let mut open: Vec<&str> = Vec::new();
        for file in files {
            let mut dirs: Vec<&str> = file.path.split('/').collect();
            let name = dirs.pop().expect("a path has a name");
            let shared = open.iter().zip(&dirs).take_while(|(a, b)| a == b).count();
            open.truncate(shared);
            for &dir in &dirs[shared..] {
                vocabulary.line(open.len(), format_args!("{dir}/"));
                vocabulary.index.add_words(dir);
                vocabulary.dirs += 1;
                open.push(dir);
            }
            let depth = dirs.len();
            match file.kind {
                FileKind::Repository => {
                    vocabulary.line(depth, format_args!("{name}/ (a repository of its own)"));
                    vocabulary.index.add_words(name);
                    vocabulary.dirs += 1;
                }
                FileKind::Link => {
                    vocabulary.line(depth, format_args!("{name} (a link)"));
                    vocabulary.index.add_words(Self::stem(name));
                    vocabulary.files += 1;
                }
                FileKind::File => {
                    vocabulary.line(depth, format_args!("{name}"));
                    vocabulary.index.add_words(Self::stem(name));
                    vocabulary.files += 1;
                    if Path::new(name).extension().is_some_and(|ext| ext == "rs") {
                        let text = read(&file.path)?;
                        vocabulary.outline(&file.path, &text, depth + 1);
                    }
                }
            }
        }
        Ok(vocabulary)
    }

    pub(crate) fn unparsed(&self) -> &[Unparsed] {
        &self.unparsed
    }

    pub(crate) fn text(&self) -> String {
        let mut text = String::from(HEADER);
        writeln!(
            text,
            "\n{} directories, {} files, {} Rust files, {} that do not parse",
            self.dirs,
            self.files,
            self.rust_files,
            self.unparsed.len()
        )
        .expect("a String takes every write");
        text.push_str("\n== Layout ==\n\n");
        text.push_str(&self.layout);
        self.index.write(&mut text);
        text
    }

    fn outline(&mut self, path: &str, text: &str, depth: usize) {
        self.rust_files += 1;
        match syn::parse_file(text) {
            Ok(file) => Outline::new(&mut self.layout, &mut self.index, path.to_owned())
                .items(&file.items, depth),
            Err(error) => {
                self.line(
                    depth,
                    format_args!("(does not parse: {})", ErrorReport::of(&error)),
                );
                self.unparsed.push(Unparsed {
                    path: path.to_owned(),
                    error,
                });
            }
        }
    }

    /// The name of the file `name` without its extension.
    fn stem(name: &str) -> &str {
        Path::new(name)
            .file_stem()
            .and_then(OsStr::to_str)
            .expect("a UTF-8 file name has a UTF-8 stem")
    }

    fn line(&mut self, depth: usize, line: fmt::Arguments<'_>) {
        outline::line(&mut self.layout, depth, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_nests_each_file_in_its_directories_and_each_item_in_its_file() {
        let file = |path: &str, kind| RepoFile {
            path: path.to_owned(),
            kind,
        };
        let files = [
            file("Cargo.toml", FileKind::File),
            file("docs/b.md", FileKind::File),
            file("docs/design/a.md", FileKind::File),
            file("source/bad.rs", FileKind::File),
            file("source/drive", FileKind::Repository),
            file("source/link.rs", FileKind::Link),
            file("source/src/lib.rs", FileKind::File),
        ];
        let mut read = Vec::new();
        let vocabulary = Vocabulary::of(&files, |path| {
            read.push(path.to_owned());
            Ok(match path {
                "source/bad.rs" => "fn 1() {}".to_owned(),
                _ => "pub struct UnitKind { id: u8 }".to_owned(),
            })
        })
        .unwrap();
        assert_eq!(read, ["source/bad.rs", "source/src/lib.rs"]);
        assert_eq!(vocabulary.unparsed().len(), 1);
        assert_eq!(vocabulary.unparsed()[0].path, "source/bad.rs");
        let text = vocabulary.text();
        let layout = text
            .split_once("== Layout ==\n\n")
            .and_then(|(_, rest)| rest.split_once("\n== Types"))
            .map(|(layout, _)| layout)
            .unwrap();
        let expected = concat!(
            "Cargo.toml\n",
            "docs/\n",
            "  b.md\n",
            "  design/\n",
            "    a.md\n",
            "source/\n",
            "  bad.rs\n",
            "    (does not parse: expected identifier)\n",
            "  drive/ (a repository of its own)\n",
            "  link.rs (a link)\n",
            "  src/\n",
            "    lib.rs\n",
            "      pub struct UnitKind\n",
            "        id: u8\n",
        );
        assert_eq!(layout, expected);
        // Four directories and a repository; six files, a link and two Rust files among them.
        assert!(text.contains("\n5 directories, 6 files, 2 Rust files, 1 that do not parse\n"));
        // The words of the layout's names count with the index's: `source` is a directory's.
        assert!(text.contains("\nsource  1\n"));
        assert!(text.contains("\nunit  1\n"));
        assert!(text.contains("\nid  1\n"));
    }
}
