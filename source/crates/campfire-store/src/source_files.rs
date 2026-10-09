use std::path::Path;

use crate::dir_entries::{DirEntries, EntryKind};
use crate::input_file::InputFile;

/// The most bytes a test reads of a source file: far past any the workspace holds.
const MAX_LEN: usize = 1 << 20;

/// A Rust source file a test read: its path from the directory it walked, its names joined by
/// `/` on every OS, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

impl SourceFile {
    /// Its production code: the code before its first test gate; none for a `tests.rs`, a
    /// `bench.rs` or a file of a `tests` directory, which hold tests alone.
    pub fn production(&self) -> Option<&str> {
        let names: Vec<&str> = self.path.split('/').collect();
        let (name, dirs) = names.split_last().expect("a source has a name");
        if dirs.contains(&"tests") || *name == "tests.rs" || *name == "bench.rs" {
            return None;
        }
        let code = ["#[cfg(test)]", "#[cfg(any(test"]
            .iter()
            .filter_map(|gate| self.text.find(gate))
            .min()
            .map_or(self.text.as_str(), |gate| &self.text[..gate]);
        Some(code)
    }
}

/// The workspace's Rust sources, as a test of the code's own rules reads them.
#[derive(Debug)]
pub struct SourceFiles;

impl SourceFiles {
    /// Each `.rs` file under `dir`, a `target` directory left out, in the order of its path.
    pub fn under(dir: &Path) -> Vec<SourceFile> {
        let mut found = Vec::new();
        SourceFiles::walk(dir, "", &mut found);
        found
    }

    fn walk(dir: &Path, at: &str, found: &mut Vec<SourceFile>) {
        let entries = DirEntries::read(dir).unwrap_or_else(|error| panic!("{error:?}"));
        for entry in entries {
            let name = entry.name.to_str().expect("a source name is UTF-8");
            let path = if at.is_empty() {
                name.to_owned()
            } else {
                format!("{at}/{name}")
            };
            match entry.kind {
                EntryKind::Dir if name != "target" => {
                    SourceFiles::walk(&dir.join(name), &path, found);
                }
                EntryKind::File if Path::new(name).extension().is_some_and(|ext| ext == "rs") => {
                    let text = InputFile::read_text(&dir.join(name), MAX_LEN)
                        .unwrap_or_else(|error| panic!("{error:?}"));
                    found.push(SourceFile { path, text });
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratch::Scratch;

    #[test]
    fn the_sources_are_each_rust_file_by_its_path_a_target_left_out() {
        let scratch = Scratch::new();
        scratch.write("src/b.rs", "b");
        scratch.write("src/a/c.rs", "c");
        scratch.write("src/notes.md", "");
        scratch.write("target/built.rs", "");
        let file = |path: &str, text: &str| SourceFile {
            path: path.to_owned(),
            text: text.to_owned(),
        };
        assert_eq!(
            SourceFiles::under(scratch.root()),
            [file("src/a/c.rs", "c"), file("src/b.rs", "b")]
        );
        // The code before the first test gate is production code; a tests file holds none.
        let gated = file("src/a.rs", "fn f() {}\n#[cfg(test)]\nmod tests;\n");
        assert_eq!(gated.production(), Some("fn f() {}\n"));
        for path in ["src/tests.rs", "src/bench.rs", "src/a/tests/mod.rs"] {
            assert_eq!(file(path, "fn f() {}").production(), None, "{path}");
        }
    }
}
