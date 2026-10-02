/// Where a unit type's name is seen. The mode's scope holds the mode's types and each avatar's,
/// by its package's name: what the mode's data and scripts name. A package's own scope holds its
/// delivery types, by their ids in it: what only its actions name. So two packages' types never
/// share a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum TypeScope {
    Mode,
    /// The package of this index, another than the mode.
    Package(u16),
}

impl TypeScope {
    /// The scope the actions of package `package` name their delivery types in: the mode's for
    /// the mode, 0, else the package's own.
    pub(crate) const fn of_package(package: u16) -> TypeScope {
        match package {
            0 => TypeScope::Mode,
            package => TypeScope::Package(package),
        }
    }
}
