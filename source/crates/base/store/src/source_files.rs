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
    /// Whether a gate of `GATES` holds its module, at its own declaration or an outer one's, so
    /// all of it is code only tests and their helpers build.
    pub gated: bool,
}

impl SourceFile {
    /// The gates of the code that only tests and their helpers build.
    const GATES: [&'static str; 2] = ["#[cfg(test)]", "#[cfg(feature = \"internals\")]"];

    /// Its production code: its text with each item a gate of `GATES` holds blanked, its doc
    /// comment and its further attributes with it, line for line, so each line keeps its number;
    /// none for a gated module's file, a `tests.rs`, a `bench.rs` or a file of a `tests`
    /// directory, which hold tests and their helpers alone. An item is found by its indentation,
    /// as rustfmt lays it out: it ends on its own line when that line ends it, else at the closing
    /// delimiter back at its indentation, or before the first line that comes back there after it
    /// ended.
    pub fn production(&self) -> Option<String> {
        let names: Vec<&str> = self.path.split('/').collect();
        let (name, dirs) = names.split_last().expect("a source has a name");
        if self.gated || dirs.contains(&"tests") || *name == "tests.rs" || *name == "bench.rs" {
            return None;
        }
        let lines: Vec<&str> = self.text.split('\n').collect();
        let mut kept = vec![true; lines.len()];
        let mut at = 0;
        while at < lines.len() {
            if !SourceFile::GATES.contains(&lines[at].trim()) {
                at += 1;
                continue;
            }
            let indent = SourceFile::indentation(lines[at]);
            let mut start = at;
            while start > 0
                && lines[start - 1].trim_start().starts_with("///")
                && SourceFile::indentation(lines[start - 1]) == indent
            {
                start -= 1;
            }
            let end = SourceFile::item_end(&lines, SourceFile::item_start(&lines, at), indent);
            kept[start..=end].fill(false);
            at = end + 1;
        }
        let code: Vec<&str> = lines
            .iter()
            .zip(kept)
            .map(|(&line, kept)| if kept { line } else { "" })
            .collect();
        Some(code.join("\n"))
    }

    /// The first line of the item the gate at `gate` holds, past its further attributes and doc
    /// comments, an attribute over several lines among them.
    fn item_start(lines: &[&str], gate: usize) -> usize {
        let indent = SourceFile::indentation(lines[gate]);
        let mut first = gate + 1;
        while first < lines.len() {
            let line = lines[first].trim();
            if line.starts_with("///") || line.starts_with("#[") && line.ends_with(']') {
                first += 1;
            } else if line.starts_with("#[") {
                let close = (first + 1..lines.len())
                    .find(|&at| {
                        SourceFile::indentation(lines[at]) == indent
                            && lines[at].trim_start().starts_with(')')
                    })
                    .expect("an attribute over several lines closes at its indentation");
                first = close + 1;
            } else {
                break;
            }
        }
        assert!(first < lines.len(), "a gate holds an item");
        first
    }

    /// The last line of the item that starts at `first`, at `indent`.
    fn item_end(lines: &[&str], first: usize, indent: usize) -> usize {
        let line = lines[first].trim_end();
        let opens = line.matches(['{', '(', '[']).count();
        let shuts = line.matches(['}', ')', ']']).count();
        if line.ends_with([';', ',']) || line.ends_with('}') && opens == shuts {
            return first;
        }
        let mut last = first;
        for (at, line) in lines.iter().enumerate().skip(first + 1) {
            let text = line.trim();
            if text.is_empty() {
                continue;
            }
            let depth = SourceFile::indentation(line);
            if depth < indent {
                return last;
            }
            if depth == indent {
                if text.starts_with(['}', ')', ']']) {
                    // A closing delimiter ends the item, but one that opens the next part, as
                    // `} else {`, goes on.
                    if !text.ends_with('{') {
                        return at;
                    }
                } else if lines[last].trim_end().ends_with([';', '}']) {
                    return last;
                }
            }
            last = at;
        }
        last
    }

    /// How many spaces open `line`.
    fn indentation(line: &str) -> usize {
        line.len() - line.trim_start_matches(' ').len()
    }

    /// The paths of the modules a gate of `GATES` holds at their declarations in this file, by
    /// the path of the module's file and of its directory: `name.rs` or `name/mod.rs` beside it
    /// for a `lib.rs`, `main.rs` or `mod.rs`, else under its own name's directory.
    fn gated_modules(&self) -> Vec<String> {
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
        let lines: Vec<&str> = self.text.split('\n').collect();
        let mut gated = Vec::new();
        for (at, line) in lines.iter().enumerate() {
            if *line != line.trim_start() || !SourceFile::GATES.contains(line) {
                continue;
            }
            let declared = lines[SourceFile::item_start(&lines, at)];
            let module = declared
                .trim_start_matches("pub ")
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub(super) ")
                .strip_prefix("mod ")
                .and_then(|rest| rest.strip_suffix(';'));
            if let Some(module) = module {
                let path = if own.is_empty() {
                    module.to_owned()
                } else {
                    format!("{own}/{module}")
                };
                gated.push(path);
            }
        }
        gated
    }
}

/// The workspace's Rust sources, as a test of the code's own rules reads them.
#[derive(Debug)]
pub struct SourceFiles;

impl SourceFiles {
    /// Each `.rs` file under `dir`, a `target` directory left out, in the order of its path, each
    /// marked when a gate holds its module.
    pub fn under(dir: &Path) -> Vec<SourceFile> {
        let mut found = Vec::new();
        SourceFiles::walk(dir, "", &mut found);
        let gated: Vec<String> = found.iter().flat_map(SourceFile::gated_modules).collect();
        for file in &mut found {
            file.gated = gated.iter().any(|module| {
                file.path == format!("{module}.rs") || file.path.starts_with(&format!("{module}/"))
            });
        }
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
                    found.push(SourceFile {
                        path,
                        text,
                        gated: false,
                    });
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

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            path: path.to_owned(),
            text: text.to_owned(),
            gated: false,
        }
    }

    #[test]
    fn the_sources_are_each_rust_file_by_its_path_a_target_left_out() {
        let scratch = Scratch::new();
        scratch.write("src/b.rs", "b");
        scratch.write("src/a/c.rs", "c");
        scratch.write("src/notes.md", "");
        scratch.write("target/built.rs", "");
        assert_eq!(
            SourceFiles::under(scratch.root()),
            [file("src/a/c.rs", "c"), file("src/b.rs", "b")]
        );
        for path in ["src/tests.rs", "src/bench.rs", "src/a/tests/mod.rs"] {
            assert_eq!(file(path, "fn f() {}").production(), None, "{path}");
        }
    }

    #[test]
    fn production_code_blanks_each_gated_item_and_keeps_each_line_at_its_number() {
        // A declaration of one line, a struct with its doc comment and attributes, a statement
        // in a function, an item whose attribute spans lines, a function whose signature spans
        // lines, a statement over several lines, and an `if` with its `else`.
        let text = concat!(
            "mod a;\n#[cfg(feature = \"internals\")]\nmod b;\nmod c;\n",
            "/// Its helpers.\n#[cfg(test)]\n#[derive(Debug)]\nstruct T {\n    x: u8,\n}\n",
            "fn f() {\n    let x = 1;\n    #[cfg(feature = \"internals\")]\n    if x == 1 {\n",
            "        g();\n    }\n    h();\n",
            "    #[cfg(test)]\n    world\n        .resource_mut::<R>()\n        .clear();\n    i();\n",
            "    #[cfg(test)]\n    if x {\n        j();\n    } else {\n        k();\n    }\n    l();\n}\n",
            "#[cfg(test)]\n#[cfg_attr(\n    debug_assertions,\n    expect(dead_code)\n)]\nfn u() {}\n",
            "#[cfg(test)]\nfn w<T>()\nwhere\n    T: Copy,\n{\n    m();\n}\nfn v() {}\n",
        );
        let code = file("src/a.rs", text).production().unwrap();
        let expected = concat!(
            "mod a;\n\n\nmod c;\n",
            "\n\n\n\n\n\n",
            "fn f() {\n    let x = 1;\n\n\n",
            "\n\n    h();\n",
            "\n\n\n\n    i();\n",
            "\n\n\n\n\n\n    l();\n}\n",
            "\n\n\n\n\n\n",
            "\n\n\n\n\n\n\nfn v() {}\n",
        );
        assert_eq!(code, expected);
        assert_eq!(code.lines().count(), text.lines().count());
    }

    #[test]
    fn a_module_a_gate_holds_is_gated_whole_its_children_with_it() {
        // `lib.rs` gates `harness`, a directory whose child it declares ungated, and `scratch`, a
        // file; `a.rs` gates `b`, its own child; `kept` and `a` stay production code.
        let scratch = Scratch::new();
        let gate = "#[cfg(feature = \"internals\")]";
        scratch.write(
            "src/lib.rs",
            format!("mod a;\n{gate}\nmod harness;\n{gate}\npub(crate) mod scratch;\nmod kept;\n"),
        );
        scratch.write("src/harness/mod.rs", "mod arena;\n");
        scratch.write("src/harness/arena.rs", "");
        scratch.write("src/scratch.rs", "");
        scratch.write("src/kept.rs", "");
        scratch.write("src/a.rs", format!("{gate}\nmod b;\n"));
        scratch.write("src/a/b.rs", "");
        let gated: Vec<(String, bool)> = SourceFiles::under(scratch.root())
            .into_iter()
            .map(|file| (file.path, file.gated))
            .collect();
        let expected = [
            ("src/a/b.rs", true),
            ("src/a.rs", false),
            ("src/harness/arena.rs", true),
            ("src/harness/mod.rs", true),
            ("src/kept.rs", false),
            ("src/lib.rs", false),
            ("src/scratch.rs", true),
        ]
        .map(|(path, gated)| (path.to_owned(), gated));
        assert_eq!(gated, expected);
    }
}
