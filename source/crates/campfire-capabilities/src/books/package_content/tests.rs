use serde::de::IntoDeserializer;
use serde::de::value::Error;

use super::*;

/// The content a table of `keys`, each an empty table, reads as.
fn read(keys: &[&str]) -> Result<PackageContent, Error> {
    let table: BTreeMap<&str, BTreeMap<&str, &str>> =
        keys.iter().map(|&key| (key, BTreeMap::new())).collect();
    PackageContent::deserialize(table.into_deserializer())
}

#[test]
fn the_keys_are_the_fields() {
    // Every field, by name: a field added breaks this pattern until `KEYS` names it too.
    let PackageContent {
        actions,
        modifiers,
        units,
        items,
    } = PackageContent::default();
    assert!(actions.is_empty() && modifiers.is_empty() && units.is_empty() && items.is_empty());
    assert_eq!(
        PackageContent::KEYS,
        ["actions", "modifiers", "units", "items"]
    );
    // Each key reads, all of them together, and a key that is no field is refused.
    for key in PackageContent::KEYS {
        assert_eq!(read(&[key]).unwrap(), PackageContent::default(), "{key}");
    }
    assert_eq!(
        read(&PackageContent::KEYS).unwrap(),
        PackageContent::default()
    );
    assert!(read(&["actions", "shop"]).is_err());
}
