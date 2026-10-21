//! The package paths of the client's files.

use campfire_common::MapName;
use campfire_values::PackagePath;

/// The client file `file` of the map `map`, `client/maps/<map>/<file>`.
pub(crate) fn of_map(map: &MapName, file: &str) -> PackagePath {
    parse(&format!("client/maps/{map}/{file}"))
}

/// The path `text` spells, one of the client's files, of names a map's name is one of.
pub(crate) fn parse(text: &str) -> PackagePath {
    PackagePath::parse(text).expect("a client file's path is a package path")
}

#[cfg(test)]
mod tests;
