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
mod tests;
