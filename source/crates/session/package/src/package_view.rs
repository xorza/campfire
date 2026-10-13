use campfire_capabilities::PackageContent;

use crate::avatar_unit::AvatarUnit;
use crate::package::Package;
use crate::package_index::PackageIndex;

/// One of a mode's packages, as every package is: its place, it, its content, and its kind.
#[derive(Debug, Clone, Copy)]
pub struct PackageView<'a> {
    pub index: PackageIndex,
    pub package: &'a Package,
    pub content: &'a PackageContent,
    pub kind: ViewKind<'a>,
}

/// The kind of one of a mode's packages.
#[derive(Debug, Clone, Copy)]
pub enum ViewKind<'a> {
    Mode,
    Avatar(&'a AvatarUnit),
    Loadout,
    Rules,
}
