use campfire_capabilities::PackageContent;
use campfire_values::PackageIndex;
use campfire_values::ScriptSource;

use crate::avatar_unit::AvatarUnit;
use crate::package::Package;

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

impl PackageView<'_> {
    /// Whether it holds `source`, a script of the mode's list: it is the mode's, or the rules
    /// package `source` names.
    pub(crate) fn holds(&self, source: &ScriptSource) -> bool {
        match (source.package(), self.kind) {
            (None, ViewKind::Mode) => true,
            (Some(name), ViewKind::Rules) => self.package.header.name == *name,
            _ => false,
        }
    }
}
