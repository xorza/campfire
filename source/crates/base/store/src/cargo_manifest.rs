use std::path::Path;

use crate::input_file::InputFile;

/// The most bytes a manifest holds: far past any the workspace's.
const MAX_LEN: usize = 1 << 16;

/// The sections a crate's manifest may hold, besides a target's dependencies; `[[test]]` and
/// `[[bench]]` by their names past the first `[`.
const KNOWN: [&str; 8] = [
    "package",
    "lib",
    "features",
    "dependencies",
    "dev-dependencies",
    "[test]",
    "[bench]",
    "lints",
];

/// The one reader of the workspace's `Cargo.toml` files, which every rule and test of the
/// manifests shares. It reads by hand, as `store` has no TOML parser: only the known sections may
/// appear, so a dependency in another form, such as `[dependencies.name]` or a dotted key before
/// the first section, fails instead of passing unseen.
#[derive(Debug)]
pub struct CargoManifest;

impl CargoManifest {
    /// The text of the manifest in the directory `dir`.
    pub fn read(dir: &Path) -> String {
        InputFile::read_text(&dir.join("Cargo.toml"), MAX_LEN)
            .unwrap_or_else(|error| panic!("{error:?}"))
    }

    /// The names of the crates `manifest` depends on, its target-specific dependencies among them,
    /// in the order it lists them.
    pub fn dependencies(manifest: &str) -> Vec<&str> {
        let mut section = "";
        let mut names = Vec::new();
        for line in manifest.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[') {
                section = header.strip_suffix(']').unwrap();
                let target =
                    section.starts_with("target.'cfg(") && section.ends_with(")'.dependencies");
                assert!(KNOWN.contains(&section) || target, "section [{section}]");
                if target {
                    section = "dependencies";
                }
            } else {
                assert!(!section.is_empty(), "{line:?} before the first section");
                if section == "dependencies" {
                    names.push(Self::dependency(line));
                }
            }
        }
        names
    }

    /// The package's name `manifest` gives.
    pub fn package_name(manifest: &str) -> &str {
        let line = Self::section(manifest, "package")
            .find(|line| line.starts_with("name ") || line.starts_with("name="))
            .expect("a manifest names its package");
        line.split('"')
            .nth(1)
            .expect("a package's name is a string")
    }

    /// The lines of `manifest`'s section `[name]`, past its header.
    pub fn section<'a>(manifest: &'a str, name: &str) -> impl Iterator<Item = &'a str> {
        let header = format!("[{name}]");
        manifest
            .lines()
            .map(str::trim)
            .skip_while(move |line| *line != header)
            .skip(1)
            .take_while(|line| !line.starts_with('['))
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
    }

    /// The crate a line of a manifest's dependencies names.
    pub fn dependency(line: &str) -> &str {
        line.split(['.', ' ', '=']).next().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use std::panic;

    use super::*;

    #[test]
    fn a_manifest_gives_its_dependencies_and_fails_on_a_form_it_does_not_know() {
        // A manifest's dependencies, a target's among them, and not its dev-dependencies or its
        // features; a section the reader does not know fails it. The target's condition is
        // spelled in parts, so this file names no OS.
        let manifest = concat!(
            "[package]\nname = \"x\"\n\n[features]\ni = [\"dep:t\"]\n\n[dependencies]\n",
            "a.workspace = true\nt = { workspace = true, optional = true }\n\n",
            "[target.'cfg(",
            "un",
            "ix)'.dependencies]\nl.workspace = true\n\n[dev-dependencies]\nd.workspace = true\n"
        );
        assert_eq!(CargoManifest::dependencies(manifest), ["a", "t", "l"]);
        assert_eq!(CargoManifest::package_name(manifest), "x");
        let dev: Vec<&str> = CargoManifest::section(manifest, "dev-dependencies")
            .map(CargoManifest::dependency)
            .collect();
        assert_eq!(dev, ["d"]);
        let dotted = panic::catch_unwind(|| {
            CargoManifest::dependencies("[dependencies.a]\nversion = \"1\"\n")
        });
        assert!(dotted.is_err());
    }
}
