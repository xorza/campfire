use campfire_package::PackageFiles;
use campfire_package_disk::PackageDir;

/// The files of the workspace's test package at `path` under `packages/`, as `test/moba/heroes/husk`
/// names one, read from its directory and checked against its index.
pub fn package_files(path: &str) -> PackageFiles {
    PackageDir::new(PackageDir::workspace(path))
        .files()
        .unwrap()
}
