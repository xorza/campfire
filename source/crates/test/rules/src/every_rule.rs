//! Every rule the workspace keeps, run as one test over one read of it, as Rust's `tidy` runs
//! the compiler's checks in one process: nextest runs each test in a process of its own, so a
//! test of each rule read and parsed the workspace once for each rule (02-engine-core.md, The
//! rules are one test).

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

use campfire_threads::Parallel;

use crate::workspace::Workspace;
use crate::{
    bans, capabilities, citations, cycles, dev_dependencies, errors, manifests, platform,
    re_exports, suffixes, test_homes, test_modules, test_support, type_names, unsafe_code, world,
};

/// A rule: its name, the path of its check, and the check, which panics on what breaks it.
#[derive(Debug, Clone, Copy)]
struct Rule {
    name: &'static str,
    check: fn(&Workspace),
}

/// Each of `checks`, named by its path.
macro_rules! rules {
    ($($check:path),* $(,)?) => {
        [$(Rule { name: stringify!($check), check: $check }),*]
    };
}

const RULES: [Rule; 43] = rules![
    bans::each_banned_call_goes_through_its_proxy_alone,
    bans::a_crates_clippy_toml_is_the_workspaces_with_its_own_entries,
    bans::no_tick_reads_removals_that_age_with_the_app,
    bans::each_crate_a_match_runs_on_bans_floats,
    bans::no_crate_a_match_runs_on_reads_the_wall_clock,
    capabilities::genre_words::no_capability_names_a_genres_words,
    capabilities::lookups::a_name_is_looked_up_only_by_a_script_call_or_the_load,
    capabilities::roles::each_role_of_a_module_takes_the_modules_name,
    capabilities::roles::a_book_and_a_data_table_take_their_suffix,
    capabilities::sets::a_module_names_another_modules_sets_only_where_its_sets_run,
    citations::a_citation_names_a_decision_its_document_holds,
    cycles::no_crates_modules_import_each_other_in_a_cycle,
    dev_dependencies::a_dev_dependency_adds_only_test_support,
    errors::every_error_the_workspace_writes_goes_through_its_report,
    manifests::common_and_each_proxy_depend_on_their_listed_crates_alone,
    manifests::only_common_names_the_crate_of_its_derives,
    manifests::the_design_marks_built_exactly_the_crates_of_the_workspace_with_their_kinds,
    manifests::each_crate_keeps_the_rule_of_its_kind,
    manifests::the_sim_reads_no_file_and_logs_nothing_and_the_protocol_reads_no_file,
    manifests::each_crate_takes_the_workspaces_lints,
    manifests::a_version_is_the_workspaces_and_a_feature_its_users,
    manifests::each_crates_dir_is_its_package_name_without_the_prefix,
    manifests::a_test_build_turns_internals_on_and_code_has_one_gate_for_it,
    manifests::a_required_dependency_is_named_by_production_code,
    manifests::a_dev_dependency_turns_on_internals_that_gate_code,
    manifests::the_default_members_are_the_build_that_ships,
    manifests::each_optional_dependency_is_named_by_dep_and_has_no_implicit_feature,
    manifests::an_optional_dependency_is_named_by_library_code,
    platform::only_the_platform_module_names_an_os,
    re_exports::no_crate_re_exports_another_crate,
    re_exports::no_module_re_exports_its_own_crates_item,
    suffixes::a_declared_name_keeps_its_suffix,
    suffixes::a_package_data_struct_refuses_a_field_it_does_not_hold,
    test_homes::a_test_lives_in_the_lowest_crate_that_can_hold_it,
    test_modules::an_inline_test_module_holds_at_most_150_lines_and_40_percent_of_its_file,
    test_support::a_crate_has_one_bench_target_and_its_cases_live_in_bench_files,
    test_support::a_test_takes_its_files_in_a_test_dir,
    type_names::a_type_name_keeps_one_meaning,
    type_names::a_stack_feature_names_no_campfire_crate,
    unsafe_code::only_the_listed_files_write_unsafe,
    world::a_free_function_that_takes_a_world_first_is_a_system,
    world::no_trait_is_a_test_trait_on_the_world_or_a_modules_trait_on_a_match,
    world::no_crate_names_another_crates_own_world_trait,
];

/// What a check's panic says.
fn message(panic: &(dyn Any + Send)) -> &str {
    panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .unwrap_or("a panic of no message")
}

#[test]
fn the_workspace_keeps_every_rule() {
    let workspace = Workspace::read();
    let broken: Vec<String> = Parallel::map(RULES.len(), |at| {
        let Rule { name, check } = RULES[at];
        let ran = panic::catch_unwind(AssertUnwindSafe(|| check(&workspace)));
        ran.err()
            .map(|panic| format!("{name}: {}", message(&*panic)))
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(broken.is_empty(), "{}", broken.join("\n\n"));
}
