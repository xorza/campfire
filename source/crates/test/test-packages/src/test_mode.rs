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

impl TestMode {
    /// The mode's tree within the workspace's `packages` directory: the mode, and every package
    /// it depends on.
    pub(crate) const fn tree(self) -> &'static str {
        match self {
            Self::Lane | Self::Proving | Self::Ruled | Self::Hooks | Self::Features => {
                "test/engine"
            }
            Self::Moba3v3 => "test/moba",
            Self::Skirmish => "",
        }
    }

    /// The mode's root within its tree.
    pub const fn root(self) -> &'static str {
        match self {
            Self::Lane => "modes/lane",
            Self::Proving => "modes/proving",
            Self::Moba3v3 => "modes/3v3",
            Self::Ruled => "modes/ruled",
            Self::Skirmish => "test/engine/modes/skirmish",
            Self::Hooks => "modes/hooks",
            Self::Features => "modes/features",
        }
    }

    /// The map the tests play the mode on.
    pub fn map(self) -> MapName {
        let name = match self {
            Self::Lane => "lane",
            Self::Proving => "proving",
            Self::Moba3v3 => "two_lanes",
            Self::Ruled => "stand",
            Self::Skirmish => "field",
            Self::Hooks => "arena",
            Self::Features => "square",
        };
        MapName::parse(name).expect("a test map's name")
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
