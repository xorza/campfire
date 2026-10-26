use std::collections::BTreeMap;
use std::path::Path;

use campfire_store::internals::TestDir;
use campfire_store::{DirEntries, EntryKind, InputFile};
use campfire_threads::Parallel;

use crate::source_file::{Gate, SourceFile};

/// The most bytes a test reads of a source file: far past any the workspace holds.
const MAX_LEN: usize = 1 << 20;

/// The workspace's Rust sources, as a test of the code's own rules reads them.
#[derive(Debug)]
pub(crate) struct SourceFiles;

impl SourceFiles {
    /// Each `.rs` file under `dir`, a `target` directory left out, in the order of its path, each
    /// with the gate that holds its module: `#[cfg(test)]` where any module on its way has it,
    /// else `internals` where one has that.
    pub(crate) fn under(dir: &Path) -> Vec<SourceFile> {
        let mut found = Vec::new();
        Self::walk(dir, "", &mut found);
        let mut gated: BTreeMap<String, Gate> = BTreeMap::new();
        let modules = Parallel::map(found.len(), |at| found[at].gated_modules());
        for (module, gate) in modules.into_iter().flatten() {
            let held = gated.entry(module).or_insert(gate);
            if gate == Gate::Test {
                *held = gate;
            }
        }
        for file in &mut found {
            // The modules that hold it: its own file's, and each of its directories'.
            let own = file
                .path
                .strip_suffix(".rs")
                .expect("a source is a `.rs` file");
            let dirs = file.path.match_indices('/').map(|(at, _)| &file.path[..at]);
            let gates = dirs
                .chain([own])
                .filter_map(|module| gated.get(module).copied());
            file.gate = gates.min_by_key(|&gate| gate != Gate::Test);
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
                    found.push(SourceFile::new(path, text));
                }
                _ => {}
            }
        }
    }
}

#[test]
fn a_module_a_gate_holds_is_gated_whole_its_children_with_it() {
    // `lib.rs` gates `harness`, a directory whose child it declares ungated, and `test_dir`, a
    // file, by `internals`, and `fake` and `line`, whose gate shares its line, by `test`;
    // `a.rs` gates `b`, its own child, and `harness/mod.rs` its `checks` by `test`, which wins over
    // the outer `internals`; `kept` and `a` stay production code. A gate in a raw string gates
    // nothing, and a file that is no `.rs` or is under `target` is not listed.
    let test_dir = TestDir::new();
    let gate = "#[cfg(feature = \"internals\")]";
    let test = "#[cfg(test)]";
    test_dir.write(
        "src/lib.rs",
        format!(
            "mod a;\n{gate}\nmod harness;\n{gate}\npub(crate) mod test_dir;\nmod kept;\n{test}\nmod fake;\n{test} mod line;\nconst RAW: &str = r\"\n{test}\nmod kept;\n\";\n"
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
    test_dir.write("src/line.rs", "");
    test_dir.write("src/a.rs", format!("{gate}\nmod b;\n"));
    test_dir.write("src/a/b.rs", "");
    test_dir.write("src/notes.md", "");
    test_dir.write("target/built.rs", "");
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
        ("src/line.rs", test),
        ("src/test_dir.rs", internals),
    ]
    .map(|(path, gate)| (path.to_owned(), gate));
    assert_eq!(gates, expected);
}
