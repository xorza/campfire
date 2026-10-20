use campfire_capabilities::{PackageContent, Release};
use campfire_codec::TomlTable;
use campfire_declarations::UnitDefaults;
use serde::de::Error;

/// A data file's table, of which a package's content is a part.
#[derive(Debug)]
pub(crate) struct ContentTable;

impl ContentTable {
    /// Takes a package's content's keys out of `table`, a data file's, and reads them, `defaults`
    /// under its unit types' sections; the rest of the table stays.
    pub(crate) fn take<E: Error>(
        table: &mut TomlTable,
        defaults: &UnitDefaults,
    ) -> Result<PackageContent, E> {
        let readers = Release::sections();
        let content = table.take_keys(PackageContent::keys(&readers));
        content
            .read_with(PackageContent::seed(&readers, defaults))
            .map_err(E::custom)
    }
}
