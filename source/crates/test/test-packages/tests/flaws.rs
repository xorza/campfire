//! The flaws of the test packages that a load refuses, and the one check of each table of them.

use campfire_package::{LoadError, LoadProblem, ModePackages, PackageRef};
use campfire_test_packages::Edit;
use campfire_threads::Parallel;

/// A flaw of the test packages: the edit that makes it, in its file, the more edits it needs,
/// each to a file, the package whose load it fails, and whether a problem is the one it gives.
#[derive(Debug)]
pub(crate) struct Flaw {
    pub(crate) file: &'static str,
    pub(crate) edit: Edit<'static>,
    pub(crate) also: &'static [(&'static str, Edit<'static>)],
    pub(crate) package: &'static str,
    pub(crate) refused: fn(&LoadProblem) -> bool,
}

impl Flaw {
    /// The flaw of the one `edit` to `file`.
    pub(crate) const fn of(
        file: &'static str,
        edit: Edit<'static>,
        package: &'static str,
        refused: fn(&LoadProblem) -> bool,
    ) -> Self {
        Self {
            file,
            edit,
            also: &[],
            package,
            refused,
        }
    }
}

/// Each of `flaws` fails `load` of its edits, in its package, by its name or its directory, with
/// its problem; the flaws load on every core (02-engine-core.md, A sweep runs on every core).
pub(crate) fn fails_each(
    flaws: &[Flaw],
    load: impl Fn(&[(&'static str, Edit<'static>)]) -> Result<ModePackages, LoadError> + Sync,
) {
    Parallel::map(flaws.len(), |at| {
        let flaw = &flaws[at];
        let mut edits = vec![(flaw.file, flaw.edit)];
        edits.extend_from_slice(flaw.also);
        let Err(error) = load(&edits) else {
            panic!("{flaw:?} loads");
        };
        let LoadError { package, problem } = &error;
        let named = match package {
            PackageRef::Name(name) => name == flaw.package,
            PackageRef::Dir(dir) => dir.ends_with(flaw.package),
            PackageRef::PackageFingerprint(_) => false,
        };
        assert!(named && (flaw.refused)(problem), "{flaw:?}: {error:?}");
    });
}
