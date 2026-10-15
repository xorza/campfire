use crate::package_index::PackageIndex;

/// Where a unit type's name is seen. The mode's scope holds the mode's types and each avatar's,
/// by its package's name: what the mode's data and scripts name. A package's own scope holds its
/// delivery types, by their ids in it: what only its actions name. So two packages' types never
/// share a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypeScope {
    Mode,
    /// The package of this index, another than the mode.
    Package(PackageIndex),
}

impl TypeScope {
    /// The scope the actions of package `package` name their delivery types in: the mode's for
    /// the mode, 0, else the package's own.
    pub const fn of_package(package: PackageIndex) -> TypeScope {
        if package.get() == PackageIndex::MODE.get() {
            TypeScope::Mode
        } else {
            TypeScope::Package(package)
        }
    }
}
