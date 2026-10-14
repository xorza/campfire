use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use campfire_codec::Toml;
use campfire_values::PackagePath;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::model_parts::ModelParts;
use crate::zero_hour::{SkipReason, SkippedModel};

/// The models of an import: each converted once, at `client/models/<name>.glb`, with the material
/// files its shaders need, through `write`; the parts each one's looks read; the models the
/// objects name that the package lacks; and the textures the models name that no archive holds.
#[derive(Debug)]
pub(crate) struct ModelWriter<'i, W> {
    assets: ModelAssets<'i>,
    write: W,
    parts: BTreeMap<String, Option<ModelParts>>,
    skipped: Vec<SkippedModel>,
    missing_textures: BTreeSet<String>,
}

/// What the models of an import lacked: the models the package lacks, and the textures no
/// archive holds.
#[derive(Debug)]
pub(crate) struct ModelReport {
    pub(crate) skipped: Vec<SkippedModel>,
    pub(crate) missing_textures: Vec<String>,
}

impl<'i, W: FnMut(&str, &[u8]) -> Result<(), ZeroHourError>> ModelWriter<'i, W> {
    pub(crate) const fn new(assets: ModelAssets<'i>, write: W) -> ModelWriter<'i, W> {
        ModelWriter {
            assets,
            write,
            parts: BTreeMap::new(),
            skipped: Vec::new(),
            missing_textures: BTreeSet::new(),
        }
    }

    /// The parts of the model `name`, in lowercase, which it converts and writes the first time
    /// it is named; none for one that draws nothing, or that the package lacks, which the report
    /// names.
    pub(crate) fn parts(&mut self, name: &str) -> Result<Option<&ModelParts>, ZeroHourError> {
        let parts = match self.parts.entry(name.to_owned()) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(new) => {
                let converted = ModelWriter::convert(
                    name,
                    &mut self.assets,
                    &mut self.write,
                    &mut self.skipped,
                    &mut self.missing_textures,
                )?;
                new.insert(converted)
            }
        };
        Ok(parts.as_ref())
    }

    /// Writes the package's file at `path`.
    pub(crate) fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), ZeroHourError> {
        (self.write)(path, bytes)
    }

    pub(crate) fn report(self) -> ModelReport {
        ModelReport {
            skipped: self.skipped,
            missing_textures: self.missing_textures.into_iter().collect(),
        }
    }

    /// Converts the model `name` and writes its files; its parts, or `None` if it draws nothing
    /// or is skipped, which `skipped` gains.
    fn convert(
        name: &str,
        assets: &mut ModelAssets<'_>,
        write: &mut W,
        skipped: &mut Vec<SkippedModel>,
        missing_textures: &mut BTreeSet<String>,
    ) -> Result<Option<ModelParts>, ZeroHourError> {
        let reason = match ModelImport::convert(name, assets)? {
            Converted::Model(import) => {
                write(path(name).as_str(), &import.glb.encode())?;
                for material in &import.materials {
                    let text = Toml::write(&material.file);
                    write(
                        &format!("client/materials/{}.toml", material.name),
                        text.as_bytes(),
                    )?;
                }
                missing_textures.extend(import.missing_textures);
                return Ok(Some(import.parts));
            }
            Converted::Nothing => return Ok(None),
            Converted::Emitter => SkipReason::Emitter,
            Converted::Missing => SkipReason::Missing,
        };
        skipped.push(SkippedModel {
            name: name.to_owned(),
            reason,
        });
        Ok(None)
    }
}

/// Where the model `name` goes: `client/models/<name>.glb`.
pub(crate) fn path(name: &str) -> PackagePath {
    PackagePath::parse(&format!("client/models/{name}.glb"))
        .expect("a model name is a package path")
}
