use campfire_capabilities::PackageContent;
use serde::Deserialize;
use serde::de::Error;

/// The keys of a data file's table that hold a package's content.
const KEYS: [&str; 4] = ["actions", "modifiers", "units", "items"];

/// Takes a package's content's keys out of `table`, a data file's, and reads them; the rest of
/// the table stays.
pub(crate) fn take<E: Error>(table: &mut toml::Table) -> Result<PackageContent, E> {
    let content: toml::Table = KEYS
        .into_iter()
        .filter_map(|key| table.remove_entry(key))
        .collect();
    PackageContent::deserialize(toml::Value::Table(content)).map_err(E::custom)
}
