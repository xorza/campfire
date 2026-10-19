use campfire_values::PackagePath;
use derive_more::Display;

use crate::zero_hour::error::NoPackagePath;
use crate::zero_hour::game_path;

/// A path inside the install's archives, as the game finds a file by it: its ASCII lowercase,
/// with `\` between names, whatever the OS. Not an OS path, so `std::path::Path`, whose separators
/// differ by OS, never reads one.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ArchivePath(String);

impl ArchivePath {
    /// Where the texture at this path goes in the package: `client/textures/` and its path
    /// with `/` between names, its extension `.ktx2`; an error for a name no package path holds.
    pub(crate) fn texture_path(&self) -> Result<PackagePath, NoPackagePath> {
        game_path::package_path(format!(
            "client/textures/{}.ktx2",
            self.without_extension().replace('\\', "/")
        ))
    }

    /// The path the game finds `path` by: `/` read as `\`, letters folded to lowercase.
    pub(crate) fn of(path: &str) -> Self {
        Self(path.replace('/', "\\").to_ascii_lowercase())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// Its names, from the first.
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.0.split('\\')
    }

    /// The text after the last `.` of its last name, if that name has one past its first byte.
    pub(crate) fn extension(&self) -> Option<&str> {
        self.split_extension().map(|(_, extension)| extension)
    }

    /// The path up to its extension's `.`, or all of it if it has none.
    pub(crate) fn without_extension(&self) -> &str {
        self.split_extension().map_or(&self.0, |(stem, _)| stem)
    }

    fn split_extension(&self) -> Option<(&str, &str)> {
        let name_at = self.0.rfind('\\').map_or(0, |at| at + 1);
        let dot = self.0[name_at..].rfind('.').filter(|&at| at > 0)?;
        let (stem, dotted) = self.0.split_at(name_at + dot);
        Some((stem, &dotted[1..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_folds_case_and_reads_its_names_and_extension_alike_on_every_os() {
        let path = ArchivePath::of("Maps/Alpine.Assault\\Alpine Assault.MAP");
        assert_eq!(path.as_str(), "maps\\alpine.assault\\alpine assault.map");
        assert_eq!(
            path.names().collect::<Vec<_>>(),
            ["maps", "alpine.assault", "alpine assault.map"]
        );
        assert_eq!(path.extension(), Some("map"));
        assert_eq!(
            path.without_extension(),
            "maps\\alpine.assault\\alpine assault"
        );
        // A dot in a directory is no extension, nor is a name's leading dot.
        for bare in ["art\\v1.2\\readme", "art\\.hidden", "plain"] {
            let path = ArchivePath::of(bare);
            assert_eq!((path.extension(), path.without_extension()), (None, bare));
        }
        assert_eq!(ArchivePath::of("a.tar.gz").extension(), Some("gz"));
    }
}
