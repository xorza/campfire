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
    /// The gate that holds its module, at its own declaration or an outer one's, so all of it is
    /// code only tests, or tests and their helpers, build; none for code every build holds.
    pub gate: Option<Gate>,
}

/// A gate of code that only tests and their helpers build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    /// `#[cfg(test)]`: a crate's own tests alone.
    Test,
    /// `#[cfg(feature = "internals")]`: the helpers of any crate's tests and benches.
    Internals,
}

impl Gate {
    /// Each gate, by the line that writes it.
    const LINES: [(&'static str, Self); 2] = [
        ("#[cfg(test)]", Self::Test),
        ("#[cfg(feature = \"internals\")]", Self::Internals),
    ];

    /// The gate `line` writes, if it writes one alone.
    fn of(line: &str) -> Option<Self> {
        let line = line.trim();
        Self::LINES
            .into_iter()
            .find_map(|(text, gate)| (text == line).then_some(gate))
    }
}

impl SourceFile {
    /// Its production code: its text with each item a gate holds blanked, its doc comment and
    /// its further attributes with it, line for line, so each line keeps its number; none for a
    /// gated module's file, a `tests.rs`, a `bench.rs` or a file of a `tests` directory, which
    /// hold tests and their helpers alone. An item is found by its indentation, as rustfmt lays
    /// it out: it ends on its own line when that line ends it, else at the closing delimiter back
    /// at its indentation, or before the first line that comes back there after it ended.
    pub fn production(&self) -> Option<String> {
        let (name, dirs) = self.names();
        if self.gate.is_some()
            || dirs.contains(&"tests")
            || name == "tests.rs"
            || name == "bench.rs"
        {
            return None;
        }
        Some(self.blanked(&[Gate::Test, Gate::Internals]))
    }

    /// Its library code, what the crate's build with `internals` holds: its text with each item
    /// `#[cfg(test)]` holds blanked, as `production` blanks one; none for a module's file that
    /// gate holds, a `tests.rs` or a file of a `tests` directory. A `bench.rs` is library code,
    /// as `internals` builds it.
    pub fn library(&self) -> Option<String> {
        let (name, dirs) = self.names();
        if self.gate == Some(Gate::Test) || dirs.contains(&"tests") || name == "tests.rs" {
            return None;
        }
        Some(self.blanked(&[Gate::Test]))
    }

    /// Its file's name, and the directories its path holds it in.
    fn names(&self) -> (&str, Vec<&str>) {
        let mut names: Vec<&str> = self.path.split('/').collect();
        let name = names.pop().expect("a source has a name");
        (name, names)
    }

    /// Its text with each item one of `gates` holds blanked, line for line.
    fn blanked(&self, gates: &[Gate]) -> String {
        let lines: Vec<&str> = self.text.split('\n').collect();
        let mut kept = vec![true; lines.len()];
        let mut at = 0;
        while at < lines.len() {
            if !Gate::of(lines[at]).is_some_and(|gate| gates.contains(&gate)) {
                at += 1;
                continue;
            }
            let indent = Self::indentation(lines[at]);
            let mut start = at;
            while start > 0
                && lines[start - 1].trim_start().starts_with("///")
                && Self::indentation(lines[start - 1]) == indent
            {
                start -= 1;
            }
            let end = Self::item_end(&lines, Self::item_start(&lines, at), indent);
            kept[start..=end].fill(false);
            at = end + 1;
        }
        let code: Vec<&str> = lines
            .iter()
            .zip(kept)
            .map(|(&line, kept)| if kept { line } else { "" })
            .collect();
        code.join("\n")
    }

    /// The first line of the item the gate at `gate` holds, past its further attributes and doc
    /// comments, an attribute over several lines among them.
    fn item_start(lines: &[&str], gate: usize) -> usize {
        let indent = Self::indentation(lines[gate]);
        let mut first = gate + 1;
        while first < lines.len() {
            let line = lines[first].trim();
            if line.starts_with("///") || line.starts_with("#[") && line.ends_with(']') {
                first += 1;
            } else if line.starts_with("#[") {
                let close = (first + 1..lines.len())
                    .find(|&at| {
                        Self::indentation(lines[at]) == indent
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
            let depth = Self::indentation(line);
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

    /// The paths of the modules a gate holds at their declarations in this file, each with its
    /// gate, by the path of the module's file and of its directory: `name.rs` or `name/mod.rs`
    /// beside it for a `lib.rs`, `main.rs` or `mod.rs`, else under its own name's directory.
    fn gated_modules(&self) -> Vec<(String, Gate)> {
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
            let Some(gate) = Gate::of(line).filter(|_| *line == line.trim_start()) else {
                continue;
            };
            let declared = lines[Self::item_start(&lines, at)];
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
                gated.push((path, gate));
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
    /// with the gate that holds its module: `#[cfg(test)]` where any module on its way has it,
    /// else `internals` where one has that.
    pub fn under(dir: &Path) -> Vec<SourceFile> {
        let mut found = Vec::new();
        Self::walk(dir, "", &mut found);
        let gated: Vec<(String, Gate)> = found.iter().flat_map(SourceFile::gated_modules).collect();
        for file in &mut found {
            let holds = |module: &str| {
                file.path == format!("{module}.rs") || file.path.starts_with(&format!("{module}/"))
            };
            let gates = gated.iter().filter(|(module, _)| holds(module));
            file.gate = gates
                .map(|&(_, gate)| gate)
                .min_by_key(|&gate| gate != Gate::Test);
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
                    Self::walk(&dir.join(name), &path, found);
                }
                EntryKind::File if Path::new(name).extension().is_some_and(|ext| ext == "rs") => {
                    let text = InputFile::read_text(&dir.join(name), MAX_LEN)
                        .unwrap_or_else(|error| panic!("{error:?}"));
                    found.push(SourceFile {
                        path,
                        text,
                        gate: None,
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
    use crate::test_dir::TestDir;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            path: path.to_owned(),
            text: text.to_owned(),
            gate: None,
        }
    }

    #[test]
    fn the_sources_are_each_rust_file_by_its_path_a_target_left_out() {
        let test_dir = TestDir::new();
        test_dir.write("src/b.rs", "b");
        test_dir.write("src/a/c.rs", "c");
        test_dir.write("src/notes.md", "");
        test_dir.write("target/built.rs", "");
        assert_eq!(
            SourceFiles::under(test_dir.root()),
            [file("src/a/c.rs", "c"), file("src/b.rs", "b")]
        );
        for path in ["src/tests.rs", "src/bench.rs", "src/a/tests/mod.rs"] {
            assert_eq!(file(path, "fn f() {}").production(), None, "{path}");
        }
        // A bench is library code, as `internals` builds it; a test's file is not.
        let library = |path| file(path, "fn f() {}").library();
        assert_eq!(library("src/bench.rs").as_deref(), Some("fn f() {}"));
        assert_eq!(library("src/tests.rs"), None);
        assert_eq!(library("src/a/tests/mod.rs"), None);
        let gated = |gate| SourceFile {
            gate: Some(gate),
            ..file("src/a.rs", "fn f() {}")
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
        // The library keeps what `internals` holds, and blanks the rest alike.
        let library = file("src/a.rs", text).library().unwrap();
        let expected = concat!(
            "mod a;\n#[cfg(feature = \"internals\")]\nmod b;\nmod c;\n",
            "\n\n\n\n\n\n",
            "fn f() {\n    let x = 1;\n    #[cfg(feature = \"internals\")]\n    if x == 1 {\n",
            "        g();\n    }\n    h();\n",
            "\n\n\n\n    i();\n",
            "\n\n\n\n\n\n    l();\n}\n",
            "\n\n\n\n\n\n",
            "\n\n\n\n\n\n\nfn v() {}\n",
        );
        assert_eq!(library, expected);
    }

    #[test]
    fn a_module_a_gate_holds_is_gated_whole_its_children_with_it() {
        // `lib.rs` gates `harness`, a directory whose child it declares ungated, and `scratch`, a
        // file, by `internals`, and `fake` by `test`; `a.rs` gates `b`, its own child, and
        // `harness/mod.rs` its `checks` by `test`, which wins over the outer `internals`; `kept`
        // and `a` stay production code.
        let test_dir = TestDir::new();
        let gate = "#[cfg(feature = \"internals\")]";
        let test = "#[cfg(test)]";
        test_dir.write(
            "src/lib.rs",
            format!(
                "mod a;\n{gate}\nmod harness;\n{gate}\npub(crate) mod test_dir;\nmod kept;\n{test}\nmod fake;\n"
            ),
        );
        test_dir.write(
            "src/harness/mod.rs",
            format!("mod arena;\n{test}\nmod checks;\n"),
        );
        test_dir.write("src/harness/arena.rs", "");
        test_dir.write("src/harness/checks.rs", "");
        test_dir.write("src/test_dir.rs", "");
        test_dir.write("src/kept.rs", "");
        test_dir.write("src/fake.rs", "");
        test_dir.write("src/a.rs", format!("{gate}\nmod b;\n"));
        test_dir.write("src/a/b.rs", "");
        let gates: Vec<(String, Option<Gate>)> = SourceFiles::under(test_dir.root())
            .into_iter()
            .map(|file| (file.path, file.gate))
            .collect();
        let (internals, test) = (Some(Gate::Internals), Some(Gate::Test));
        let expected = [
            ("src/a/b.rs", internals),
            ("src/a.rs", None),
            ("src/fake.rs", test),
            ("src/harness/arena.rs", internals),
            ("src/harness/checks.rs", test),
            ("src/harness/mod.rs", internals),
            ("src/kept.rs", None),
            ("src/lib.rs", None),
            ("src/test_dir.rs", internals),
        ]
        .map(|(path, gate)| (path.to_owned(), gate));
        assert_eq!(gates, expected);
    }
}
