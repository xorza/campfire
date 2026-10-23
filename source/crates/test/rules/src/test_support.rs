//! Where the tests' files and the benches live (02-engine-core.md, A test's files are on the
//! tests' file system; 02-engine-core.md, One bench target for each crate).

use crate::allowlist::Allowlist;
use crate::source_file::SourceFile;
use crate::workspace::Workspace;

/// The bench targets past a crate's one, each by its crate's directory and its name: the
/// allocation bench, whose counting allocator is its whole binary's.
const MORE_BENCH_TARGETS: [(&str, &str); 1] = [("crates/test/harness", "allocations")];

/// The files past a `bench.rs` that define bench cases: the measure crate's kinds, which run each
/// case of a kind as the benches' tool.
const CASES_ELSEWHERE: [&str; 1] = ["crates/test/measure/src/kind.rs"];

/// The calls that define a bench case.
/// Spelled in parts, so that this file's own list holds none of them.
const CASE_CALLS: [&str; 3] = [
    concat!("bench_", "function"),
    concat!("benchmark_", "group"),
    concat!("bench_", "with_input"),
];

/// Whether the source at `path` is a bench's: a `bench.rs`, a file of a `bench` directory, or a
/// target's wiring under `benches/`.
fn is_bench(path: &str) -> bool {
    path.ends_with("/bench.rs") || path.contains("/bench/") || path.contains("/benches/")
}

/// The lines of `file` that are test code, with their numbers: each of a whole test file's, and
/// each that its production view blanks, an inline test's among them.
fn test_lines(file: &SourceFile) -> Vec<(usize, &str)> {
    let lines = file
        .text
        .lines()
        .enumerate()
        .map(|(at, line)| (at + 1, line));
    match file.production() {
        None => lines.collect(),
        Some(production) => lines
            .zip(production.lines().chain(std::iter::repeat("")))
            .filter(|((_, line), kept)| kept.trim().is_empty() && !line.trim().is_empty())
            .map(|(line, _)| line)
            .collect(),
    }
}

#[test]
fn a_crate_has_one_bench_target_and_its_cases_live_in_bench_files() {
    let workspace = Workspace::read();
    let more = Allowlist::new(&MORE_BENCH_TARGETS);
    let mut found = Vec::new();
    for krate in workspace.crates() {
        let benches = krate.manifest.bench_names();
        let extra = benches.iter().skip(1).filter(|name| {
            !more.allows(|&(dir, bench)| dir == krate.path && bench == name.as_str())
        });
        found.extend(extra.map(|name| format!("{}: a second bench target {name}", krate.path)));
    }
    let elsewhere = Allowlist::new(&CASES_ELSEWHERE);
    for file in workspace.sources() {
        let defines = CASE_CALLS.iter().any(|call| file.text.contains(call));
        if defines && !is_bench(&file.path) && !elsewhere.allows(|path| *path == file.path) {
            found.push(format!("{}: a bench case outside a bench.rs", file.path));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    assert_eq!(more.unused(), Vec::<&(&str, &str)>::new());
    assert_eq!(elsewhere.unused(), Vec::<&&str>::new());
    assert!(is_bench("crates/a/src/x/bench.rs") && is_bench("crates/a/benches/a.rs"));
    assert!(!is_bench("crates/a/src/benchmark.rs"));
}

#[test]
fn a_test_takes_its_files_in_a_test_dir() {
    let workspace = Workspace::read();
    let read = concat!("env::", "temp_dir");
    let found: Vec<String> = workspace
        .sources()
        .iter()
        .filter(|file| !file.path.starts_with("crates/base/store/src/test_dir/"))
        .flat_map(|file| {
            test_lines(file)
                .into_iter()
                .filter(|(_, line)| line.contains(read))
                .map(|(at, _)| format!("{}:{at}", file.path))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(found, Vec::<String>::new());
    // A whole test file's lines are a test's, and an inline test module's; production's are not.
    let code = "fn f() {}\n\n#[cfg(test)]\nmod tests {\n    fn t() {}\n}\n";
    let file = SourceFile::new("crates/a/src/a.rs", code);
    let lines = test_lines(&file);
    assert_eq!(lines.first(), Some(&(3, "#[cfg(test)]")));
    assert!(lines.iter().all(|&(at, _)| at >= 3));
    let whole = SourceFile::new("crates/a/src/a/tests.rs", code);
    assert_eq!(test_lines(&whole).len(), 6);
}
