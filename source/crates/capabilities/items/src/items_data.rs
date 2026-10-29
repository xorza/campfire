use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{BookCx, BookError, NameSets, Section, SectionTable};
use campfire_script::{ApiVersion, Status};
use campfire_values::{ItemId, Named, PackageIndex};
use serde::Deserialize;

use crate::item_data::ItemData;

/// A package's `[items]`: its item types, by id; the load lets only the mode hold them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct ItemsData(pub BTreeMap<DeclaredName, ItemData>);

impl Section for ItemsData {
    const TABLE: SectionTable = SectionTable::Package;
    const KEY: &'static str = "items";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}

impl ItemsData {
    /// Adds the mode's item types to `sets`, as an action or a script names one before the item
    /// book gives their ids.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn names(cx: &BookCx<'_>, sets: &mut NameSets) -> Result<(), BookError> {
        let items = cx.package(PackageIndex::MODE).content.get::<Self>();
        sets.extend(
            ItemId::NAMES,
            items.into_iter().flat_map(|items| items.0.keys()),
        );
        Ok(())
    }
}
