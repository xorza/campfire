use std::path::PathBuf;

use campfire_common::{EnumAll, MapName};
use campfire_package::ModePackages;
use campfire_package_disk::PackageDir;

/// The modes of the workspace's test packages, each in its tree, at its root, on its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum TestMode {
    /// `test-lane`, on `lane`.
    Lane,
    /// `test-proving`, on `proving`.
    Proving,
    /// The MOBA test content's 3v3, on `two_lanes`.
    Moba3v3,
    /// `test-ruled`, on `stand`: a mode that runs its rules package's script as its own.
    Ruled,
    /// `test-skirmish`, on `field`: a mode that runs Zero Hour's rules package's script.
    Skirmish,
    /// `test-hooks`, on `arena`: the mode whose script each of the engine's tests of a mode's
    /// script writes.
    Hooks,
    /// `test-features`, on `square`: a mode of each fact that decides what a match installs.
    Features,
}

/// Where a test mode lies and plays: its tree within the workspace's `packages` directory, the
/// mode and every package it depends on, its root within that tree, and the map the tests play
/// it on.
#[derive(Debug, Clone, Copy)]
struct ModeSpec {
    tree: &'static str,
    root: &'static str,
    map: &'static str,
}

impl TestMode {
    const fn spec(self) -> ModeSpec {
        let (tree, root, map) = match self {
            Self::Lane => ("test/engine", "modes/lane", "lane"),
            Self::Proving => ("test/engine", "modes/proving", "proving"),
            Self::Moba3v3 => ("test", "moba/modes/3v3", "two_lanes"),
            Self::Ruled => ("test/engine", "modes/ruled", "stand"),
            Self::Skirmish => ("", "test/engine/modes/skirmish", "field"),
            Self::Hooks => ("test/engine", "modes/hooks", "arena"),
            Self::Features => ("test/engine", "modes/features", "square"),
        };
        ModeSpec { tree, root, map }
    }

    /// The mode's tree within the workspace's `packages` directory: the mode, and every package
    /// it depends on.
    pub(crate) const fn tree(self) -> &'static str {
        self.spec().tree
    }

    /// The mode's root within its tree.
    pub const fn root(self) -> &'static str {
        self.spec().root
    }

    /// The map the tests play the mode on.
    pub fn map(self) -> MapName {
        MapName::parse(self.spec().map).expect("a test map's name")
    }

    /// The mode's directory on disk.
    pub fn dir(self) -> PathBuf {
        PackageDir::workspace(self.tree()).join(self.root())
    }

    /// The mode's packages on its map, as the workspace holds them.
    pub fn packages(self) -> ModePackages {
        ModePackages::at(&PackageDir::new(self.dir()), Some(&self.map()))
            .expect("a test mode loads")
    }
}
