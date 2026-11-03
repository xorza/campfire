use std::collections::BTreeSet;

use crate::bans::{misplaced_exemptions, proxy_foreign_names, wall_clock_reads};
use crate::capabilities::genre_words::{genre_words, names_a_game};
use crate::capabilities::lookups::lookups;
use crate::capabilities::part_names::declared_parts;
use crate::capabilities::roles::{RoleHolder, RoleHolders};
use crate::capabilities::sets::SetNames;
use crate::cfg_condition::CfgCondition;
use crate::cycles::starts_at_super;
use crate::errors::source_messages;
use crate::manifests::{internals_conditions, internals_roots};
use crate::parsed_code::ParsedCode;
use crate::platform::os_names;
use crate::re_exports::{foreign_re_exports, inner_re_exports};
use crate::source_file::SourceFile;
use crate::source_files::{gate_breaches, imports_in_order};
use crate::suffixes::{SuffixNames, misnamed, open_data};
use crate::test_modules::{FatModule, fat_test_modules, test_gated_internals};
use crate::trait_defaults::{TraitDefaults, defaults};
use crate::type_names::declared;
use crate::world::{WorldFirst, world_first};

/// What each rule that lexes or parses a source finds in one file, each the rule's own part of
/// the file, read in one pass on one thread, which lexes and parses each view of the file's code
/// once for every rule; the rule's check then reads the findings of every file
/// (02-engine-core.md, The rules are one test).
#[derive(Debug)]
pub(crate) struct FileFindings {
    pub(crate) imports_in_order: bool,
    pub(crate) gate_breaches: Vec<usize>,
    pub(crate) declared: BTreeSet<String>,
    pub(crate) error_sources: Vec<usize>,
    pub(crate) world_first: Vec<WorldFirst>,
    pub(crate) foreign_re_exports: Vec<usize>,
    pub(crate) inner_re_exports: Vec<usize>,
    pub(crate) part_names: Vec<String>,
    pub(crate) role_holders: Vec<RoleHolder>,
    pub(crate) lookups: Vec<String>,
    pub(crate) set_names: SetNames,
    pub(crate) genre_words: Vec<String>,
    pub(crate) names_a_game: bool,
    pub(crate) suffix_names: SuffixNames,
    pub(crate) open_data: Vec<String>,
    pub(crate) misplaced_exemptions: Vec<usize>,
    pub(crate) foreign_names: Vec<&'static str>,
    pub(crate) wall_clock_reads: Vec<&'static str>,
    pub(crate) fat_test_modules: Vec<FatModule>,
    pub(crate) test_gated_internals: Vec<usize>,
    pub(crate) trait_defaults: Vec<TraitDefaults>,
    pub(crate) starts_at_super: bool,
    pub(crate) os_names: Vec<usize>,
    pub(crate) internals_conditions: Vec<CfgCondition>,
    pub(crate) internals_roots: BTreeSet<String>,
}

impl FileFindings {
    /// The findings of `file`, whose text `text` holds, lexed and parsed as a rule first asks.
    pub(crate) fn of(file: &SourceFile, text: &ParsedCode<'_>) -> Self {
        let path = file.path.as_str();
        let production = file.production().map(|code| ParsedCode::new(path, code));
        let library = file.library().map(|code| ParsedCode::new(path, code));
        let production = production.as_ref();
        Self {
            imports_in_order: imports_in_order(text),
            gate_breaches: gate_breaches(text),
            declared: declared(path, text),
            error_sources: source_messages(file, text),
            world_first: world_first(file, text),
            foreign_re_exports: foreign_re_exports(text),
            inner_re_exports: inner_re_exports(path, text),
            part_names: declared_parts(file, text),
            role_holders: of_view(path, production, RoleHolders::of),
            lookups: of_view(path, production, lookups),
            set_names: production.map_or_else(SetNames::default, |production| {
                SetNames::of(path, production)
            }),
            genre_words: of_view(path, production, genre_words),
            names_a_game: production.is_some_and(names_a_game),
            suffix_names: misnamed(text),
            open_data: open_data(file, text),
            misplaced_exemptions: misplaced_exemptions(path, text),
            foreign_names: proxy_foreign_names(path, text),
            wall_clock_reads: library.as_ref().map_or_else(Vec::new, wall_clock_reads),
            fat_test_modules: fat_test_modules(text),
            test_gated_internals: test_gated_internals(text),
            trait_defaults: of_view(path, production, defaults),
            starts_at_super: production.is_some_and(starts_at_super),
            os_names: os_names(text),
            internals_conditions: internals_conditions(text),
            internals_roots: internals_roots(text),
        }
    }
}

/// What `find` finds in `view`, a view of the file at `path`, and nothing where the file has no
/// such view.
fn of_view<T>(
    path: &str,
    view: Option<&ParsedCode<'_>>,
    find: fn(&str, &ParsedCode<'_>) -> Vec<T>,
) -> Vec<T> {
    view.map_or_else(Vec::new, |view| find(path, view))
}
