//! Where the benches live (02-engine-core.md, One bench target for each crate).

use crate::allowlist::Allowlist;
use crate::workspace::Workspace;

/// The bench targets past a crate's one, each by its crate's directory and its name: the
/// allocation bench, whose counting allocator is its whole binary's.
const MORE_BENCH_TARGETS: [(&str, &str); 1] = [("crates/test/harness", "allocations")];

/// The files past a `bench.rs` that define bench cases: the measure crate's kinds, which run each
/// case of a kind, and its run costs, which register a run's worst and mean, as the benches' tool.
const CASES_ELSEWHERE: [&str; 2] = [
    "crates/test/measure/src/kind.rs",
    "crates/test/measure/src/run_cost.rs",
];

/// The calls that define a bench case, which a file's code names past its comments. Spelled in
/// parts, so that this file's own list holds none of them.
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

pub(crate) fn a_crate_has_one_bench_target_and_its_cases_live_in_bench_files(
    workspace: &Workspace,
) {
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
        let defines = CASE_CALLS.iter().any(|call| file.code().contains(call));
        if defines && !is_bench(&file.path) && !elsewhere.allows(|path| *path == file.path) {
            found.push(format!("{}: a bench case outside a bench.rs", file.path));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    assert_eq!(more.unused(), Vec::<&(&str, &str)>::new());
    assert_eq!(elsewhere.unused(), Vec::<&&str>::new());
}

#[test]
fn a_bench_file_is_a_bench_rs_a_bench_directory_s_or_a_targets_wiring() {
    assert!(is_bench("crates/a/src/x/bench.rs") && is_bench("crates/a/benches/a.rs"));
    assert!(!is_bench("crates/a/src/benchmark.rs"));
}
