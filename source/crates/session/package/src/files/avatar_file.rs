use campfire_capabilities::{PackageContent, Release};
use campfire_codec::TomlTable;
use campfire_declare::{UnitDefaults, UnitTypeData};
use campfire_values::MessageId;
use serde::de::{DeserializeSeed, Error};
use serde::{Deserialize, Deserializer};

use crate::files::content_table::ContentTable;

/// An avatar package's `data/avatar.toml`: the name players see, its one unit type, whose
/// fields sit at the top of the table, and its content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AvatarFile {
    /// The message of the name players see.
    pub(crate) name: MessageId,
    pub(crate) unit: UnitTypeData,
    pub(crate) content: PackageContent,
}

/// What reads an avatar's file, as serde's seed, of the mode's `defaults`, which lie under its
/// unit types' sections.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AvatarSeed<'d>(pub(crate) &'d UnitDefaults);

impl<'de> DeserializeSeed<'de> for AvatarSeed<'_> {
    type Value = AvatarFile;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<AvatarFile, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            name: MessageId,
            #[serde(flatten)]
            rest: TomlTable,
        }
        let Fields { name, mut rest } = Fields::deserialize(deserializer)?;
        let content = ContentTable::take(&mut rest, self.0)?;
        let readers = Release::sections();
        let unit = rest.read_with(UnitTypeData::seed(&readers, self.0));
        let unit = unit.map_err(D::Error::custom)?;
        Ok(AvatarFile {
            name,
            unit,
            content,
        })
    }
}
