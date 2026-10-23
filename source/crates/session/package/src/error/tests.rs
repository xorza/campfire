use std::error::Error;

use campfire_actions::ActionProblem;
use campfire_common::DeclaredName;
use campfire_declarations::{BookError, BookPlace, Problem};
use campfire_package_format::ContentError;
use campfire_values::{PackageIndex, PackagePath};

use super::*;
use crate::error::locale_problem::LocaleProblem;
use crate::error::place::Place;
use crate::error::script_problem::ScriptProblem;

/// The message of `error` and of each error under it, down to the root.
fn chain(error: &(dyn Error + 'static)) -> Vec<String> {
    let mut messages = vec![error.to_string()];
    let mut source = error.source();
    while let Some(cause) = source {
        messages.push(cause.to_string());
        source = cause.source();
    }
    messages
}

/// The problem of a module's rule that `error` gives.
fn rule(error: ActionProblem) -> Problem {
    match BookError::rule(PackageIndex::MODE, BookPlace::Map, error) {
        BookError::Rule { problem, .. } => problem,
        other => panic!("a rule's error, not {other:?}"),
    }
}

#[test]
fn a_load_error_names_its_package_and_its_problem_and_the_error_under_it() {
    let hero = PackageName::new("hero").unwrap();
    let error = |problem| LoadError::of(&hero, problem);
    let path = |text| PackagePath::parse(text).unwrap();
    // A problem that holds an error adds its own step, and one that only holds one is the
    // error itself, so no step shows twice.
    let cases = [
        (
            LoadProblem::Rule {
                at: Place::Entry {
                    table: "actions",
                    name: DeclaredName::new("haste").unwrap(),
                },
                problem: rule(ActionProblem::SlottedRanks),
            },
            &[
                "package hero",
                "[actions.haste]",
                "placed in slot kinds of different ranks",
            ][..],
        ),
        (
            LoadProblem::Content(ContentError::Changed { path: path("x") }),
            &["package hero", "x differs from the package's index"],
        ),
        (
            LoadProblem::Script {
                path: path("scripts/a.rhai"),
                problem: ScriptProblem::Missing,
            },
            &["package hero", "scripts/a.rhai", "named, but not held"],
        ),
        (
            LoadProblem::Locale {
                path: path("en.ftl"),
                problem: LocaleProblem::FileName,
            },
            &[
                "package hero",
                "en.ftl",
                "not <language>.ftl, its language in its canonical spelling",
            ],
        ),
        (
            LoadProblem::WrongKind,
            &["package hero", "not a package of the kind its place needs"],
        ),
    ];
    for (problem, messages) in cases {
        assert_eq!(chain(&error(problem)), messages);
    }
}
