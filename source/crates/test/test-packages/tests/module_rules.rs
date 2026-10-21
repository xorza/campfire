//! What a load problem of a module's rule is, as the package tests read it.

use campfire_common::DeclaredName;
use campfire_package::{LoadProblem, Place};

/// Whether `problem` is data at its place that breaks the rule of a module whose error `test`
/// takes.
pub(super) fn rule_of<E: 'static>(
    problem: &LoadProblem,
    test: impl Fn(&Place, &E) -> bool,
) -> bool {
    matches!(problem, LoadProblem::Rule { at, problem } if problem.get().is_some_and(|error| test(at, error)))
}

/// Whether `problem` is data that breaks the rule of a module whose error is `error`.
pub(super) fn rule<E: PartialEq + 'static>(problem: &LoadProblem, error: &E) -> bool {
    rule_of(problem, |_, found: &E| found == error)
}

/// Whether `problem` is the data at `at` that breaks the rule of a module whose error is `error`.
pub(super) fn rule_at<E: PartialEq + 'static>(
    problem: &LoadProblem,
    at: &Place,
    error: &E,
) -> bool {
    rule_of(problem, |place, found: &E| place == at && found == error)
}

/// The place of the entry `name` of the table `table`.
pub(super) fn entry(table: &'static str, name: &str) -> Place {
    Place::Entry {
        table,
        name: DeclaredName::new(name).unwrap(),
    }
}

/// The place of the unit type `name`.
pub(super) fn unit_type(name: &str) -> Place {
    Place::UnitType(DeclaredName::new(name).unwrap())
}
