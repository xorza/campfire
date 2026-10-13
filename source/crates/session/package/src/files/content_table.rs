use campfire_capabilities::PackageContent;
use serde::Deserialize;
use serde::de::Error;

/// A data file's table, of which a package's content is a part.
#[derive(Debug)]
pub(crate) struct ContentTable;

impl ContentTable {
    /// Takes a package's content's keys out of `table`, a data file's, and reads them; the rest of
    /// the table stays.
    pub(crate) fn take<E: Error>(table: &mut toml::Table) -> Result<PackageContent, E> {
        let content: toml::Table = PackageContent::KEYS
            .into_iter()
            .filter_map(|key| table.remove_entry(key))
            .collect();
        PackageContent::deserialize(toml::Value::Table(content)).map_err(E::custom)
    }
}
