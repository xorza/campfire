use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The images of Zero Hour's `MappedImages\` files, as `ImageCollection` reads them, by their
/// names in lower case, as `findImageByName` finds a name of any case; a file read later replaces
/// an image of the same name.
#[derive(Debug, Default)]
pub(crate) struct MappedImageIni {
    images: BTreeMap<String, MappedImage>,
}

/// What the import reads of an image: its texture's file name, its rectangle of the texture, its
/// `Coords` of `Left`, `Top`, `Right` and `Bottom` in pixels, and whether it is stored turned a
/// quarter clockwise, its `Status` of `ROTATED_90_CLOCKWISE`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct MappedImage {
    pub(crate) texture: String,
    pub(crate) coords: [i32; 4],
    pub(crate) rotated: bool,
}

impl MappedImageIni {
    /// Reads one INI file, adding its images to those read before, each read again replacing the
    /// one before, as `INI_LOAD_OVERWRITE` does.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("mappedimage"))
        {
            let block = block?;
            let mut image = MappedImage::default();
            for line in &block.fields {
                match line.word() {
                    "texture" => line.value()?.clone_into(&mut image.texture),
                    "coords" => {
                        image.coords = line.int_coord(["Left", "Top", "Right", "Bottom"])?;
                    }
                    "status" => {
                        image.rotated = line.tokens[1..]
                            .iter()
                            .any(|flag| flag.eq_ignore_ascii_case("rotated_90_clockwise"));
                    }
                    _ => {}
                }
            }
            self.images
                .insert(block.name()?.to_ascii_lowercase(), image);
        }
        Ok(())
    }

    /// The image `name`, of any case; none for a name no file gives.
    pub(crate) fn get(&self, name: &str) -> Option<&MappedImage> {
        self.images.get(&name.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests;
