//! The package paths of the files the import makes of the game's names, and the links between
//! them.

use std::fmt::Write as _;

use campfire_values::PackagePath;

use crate::zero_hour::error::NoPackagePath;

/// `text`, a path the import made of the game's names; an error for one no package path spells,
/// as a name of `:`, `..` or `aux` gives.
pub(crate) fn package_path(text: String) -> Result<PackagePath, NoPackagePath> {
    PackagePath::parse(&text).ok_or(NoPackagePath(text))
}

/// Where the model `name` goes: `client/models/<name>.glb`; an error for a name no package path
/// holds.
pub(crate) fn model(name: &str) -> Result<PackagePath, NoPackagePath> {
    package_path(format!("client/models/{name}.glb"))
}

/// The URI of the file at `to` from the file at `from`, as a glTF names its texture: a `..` for
/// each directory of `from` past the two paths' common ones, then the rest of `to`, each byte no
/// URI takes as itself escaped.
pub(crate) fn relative_uri(from: &PackagePath, to: &PackagePath) -> String {
    let from_dirs: Vec<&str> = from.as_str().split('/').collect();
    let from_dirs = &from_dirs[..from_dirs.len() - 1];
    let to_names: Vec<&str> = to.as_str().split('/').collect();
    let common = from_dirs
        .iter()
        .zip(&to_names[..to_names.len() - 1])
        .take_while(|(from, to)| from == to)
        .count();
    let mut uri = "../".repeat(from_dirs.len() - common);
    for byte in to_names[common..].join("/").bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            write!(uri, "%{byte:02X}").expect("a String takes any text");
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_makes_a_package_path_or_an_error_and_a_link_climbs_to_the_common_directory() {
        assert_eq!(
            package_path("client/models/avtank.glb".to_owned())
                .unwrap()
                .as_str(),
            "client/models/avtank.glb"
        );
        for refused in [
            "client/models/a:b.glb",
            "client/../x.glb",
            "client/models/aux.glb",
        ] {
            assert_eq!(
                package_path(refused.to_owned()),
                Err(NoPackagePath(refused.to_owned()))
            );
        }
        let path = |text: &str| PackagePath::parse(text).unwrap();
        let texture = path("client/textures/art/a b.ktx2");
        // From a model one directory under `client/`, one `..`; from one two under, two; from
        // the texture's own directory, none; a space escaped.
        assert_eq!(
            relative_uri(&path("client/models/m.glb"), &texture),
            "../textures/art/a%20b.ktx2"
        );
        assert_eq!(
            relative_uri(&path("client/models/deep/m.glb"), &texture),
            "../../textures/art/a%20b.ktx2"
        );
        assert_eq!(
            relative_uri(&path("client/textures/art/m.glb"), &texture),
            "a%20b.ktx2"
        );
    }
}
