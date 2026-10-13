use std::path::PathBuf;

use campfire_common::MapName;

use crate::mode_packages::ModePackages;
use crate::package_dir::PackageDir;

/// The modes of the workspace's test packages, each in its tree, at its root, on its map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestMode {
    /// `test-lane`, on `lane`.
    Lane,
    /// `test-proving`, on `proving`.
    Proving,
    /// The MOBA test content's 3v3, on `two_lanes`.
    Moba3v3,
    /// `test-ruled`, on `stand`: a mode that runs its rules package's script as its own.
    Ruled,
}

impl TestMode {
    /// The mode's tree within the workspace's `packages` directory: the mode, and every package
    /// it depends on.
    pub(crate) const fn tree(self) -> &'static str {
        match self {
            TestMode::Lane | TestMode::Proving | TestMode::Ruled => "test",
            TestMode::Moba3v3 => "test/moba",
        }
    }

    /// The mode's root within its tree.
    pub const fn root(self) -> &'static str {
        match self {
            TestMode::Lane => "modes/lane",
            TestMode::Proving => "modes/proving",
            TestMode::Moba3v3 => "modes/3v3",
            TestMode::Ruled => "modes/ruled",
        }
    }

    /// The map the tests play the mode on.
    pub fn map(self) -> MapName {
        let name = match self {
            TestMode::Lane => "lane",
            TestMode::Proving => "proving",
            TestMode::Moba3v3 => "two_lanes",
            TestMode::Ruled => "stand",
        };
        MapName::new(name).expect("a test map's name")
    }

    /// The mode's directory on disk.
    pub fn dir(self) -> PathBuf {
        PackageDir::workspace(self.tree()).join(self.root())
    }

    /// The mode's packages on its map, as the workspace holds them.
    pub fn packages(self) -> ModePackages {
        ModePackages::from_dir(&self.dir(), &self.map()).expect("a test mode loads")
    }
}
