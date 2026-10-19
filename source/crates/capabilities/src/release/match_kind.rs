use campfire_mode::ModeSetup;
use campfire_values::PackagePath;

/// What a match's build does once its modules are in, by what the match is.
#[derive(Debug)]
pub enum MatchKind<'a> {
    /// The rules alone: no mode, and no map, as a test or a bench builds them.
    Rules,
    /// A predicting client's: the map's pathing for the walkers of its units.
    Predicting,
    /// A server's or a verifier's: the packages' scripts compiled, then its mode.
    Mode(ModeStart<'a>),
}

/// What a match that runs its mode starts it from: every package's scripts, in the order a match
/// compiles them, and the mode's setup.
#[derive(Debug)]
pub struct ModeStart<'a> {
    pub scripts: &'a [PackageScript<'a>],
    pub setup: ModeSetup<'a>,
}

/// A script of a package: its path, which a failure to compile names, and its source.
#[derive(Debug, Clone, Copy)]
pub struct PackageScript<'a> {
    pub path: &'a PackagePath,
    pub source: &'a str,
}
