use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The terrain types of Zero Hour's `Terrain.ini` files, as `TerrainTypeCollection` reads them:
/// each type's texture file, by the type's name as the INI writes it, which a map's texture
/// class names exactly.
#[derive(Debug, Default)]
pub(crate) struct TerrainIni {
    textures: BTreeMap<String, String>,
}

/// The type whose fields a new type starts with.
const DEFAULT: &str = "DefaultTerrain";

impl TerrainIni {
    /// Reads one INI file, adding its types to those read before: a type read again keeps its
    /// fields and takes the new ones, and a new one starts with `DefaultTerrain`'s texture.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("terrain"))
        {
            let block = block?;
            let name = block.name()?.to_owned();
            let texture = match self.textures.get(&name) {
                Some(held) => held.clone(),
                None => self.textures.get(DEFAULT).cloned().unwrap_or_default(),
            };
            let texture = self.textures.entry(name).or_insert(texture);
            for line in &block.fields {
                if line.word() == "texture" {
                    line.value()?.clone_into(texture);
                }
            }
        }
        Ok(())
    }

    /// The texture file of the type `name`, as `Texture` gives it; none for a name no file
    /// defines.
    pub(crate) fn texture(&self, name: &str) -> Option<&str> {
        self.textures.get(name).map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
