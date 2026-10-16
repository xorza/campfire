use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use campfire_client_files::MaterialFile;
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
    converter: Converter<'i, W>,
    written: BTreeMap<String, WrittenModel>,
}

/// What a model the objects name gives their looks: the parts its looks read; nothing, as it
/// draws nothing, as a `NULL` or a collision box; or nothing, as the package lacks it, which the
/// report names.
#[derive(Debug)]
pub(crate) enum WrittenModel {
    Parts(ModelParts),
    Nothing,
    Skipped,
}

/// What converts a model and writes its files: the assets it reads, the writer, and what the
/// models lacked so far.
#[derive(Debug)]
struct Converter<'i, W> {
    assets: ModelAssets<'i>,
    write: W,
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
    pub(crate) const fn new(assets: ModelAssets<'i>, write: W) -> Self {
        ModelWriter {
            converter: Converter {
                assets,
                write,
                skipped: Vec::new(),
                missing_textures: BTreeSet::new(),
            },
            written: BTreeMap::new(),
        }
    }

    /// What the model `name`, in lowercase, gives its looks, which it converts and writes the
    /// first time it is named.
    pub(crate) fn model(&mut self, name: &str) -> Result<&WrittenModel, ZeroHourError> {
        Ok(match self.written.entry(name.to_owned()) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(new) => new.insert(self.converter.convert(name)?),
        })
    }

    /// Writes the package's file at `path`.
    pub(crate) fn write(&mut self, path: &str, bytes: &[u8]) -> Result<(), ZeroHourError> {
        (self.converter.write)(path, bytes)
    }

    pub(crate) fn report(self) -> ModelReport {
        ModelReport {
            skipped: self.converter.skipped,
            missing_textures: self.converter.missing_textures.into_iter().collect(),
        }
    }
}

impl<W: FnMut(&str, &[u8]) -> Result<(), ZeroHourError>> Converter<'_, W> {
    /// Converts the model `name` and writes its files; what it gives its looks, `skipped` gaining
    /// a model the package lacks.
    fn convert(&mut self, name: &str) -> Result<WrittenModel, ZeroHourError> {
        let reason = match ModelImport::convert(name, &mut self.assets)? {
            Converted::Model(import) => {
                (self.write)(path(name).as_str(), &import.glb.encode())?;
                for material in &import.materials {
                    let text = Toml::write(&material.file);
                    let path = MaterialFile::path(&material.name)
                        .expect("a material's name is a package path's name");
                    (self.write)(path.as_str(), text.as_bytes())?;
                }
                self.missing_textures.extend(import.missing_textures);
                return Ok(WrittenModel::Parts(import.parts));
            }
            Converted::Nothing => return Ok(WrittenModel::Nothing),
            Converted::Emitter => SkipReason::Emitter,
            Converted::Missing => SkipReason::Missing,
        };
        self.skipped.push(SkippedModel {
            name: name.to_owned(),
            reason,
        });
        Ok(WrittenModel::Skipped)
    }
}

/// Where the model `name` goes: `client/models/<name>.glb`.
pub(crate) fn path(name: &str) -> PackagePath {
    PackagePath::parse(&format!("client/models/{name}.glb"))
        .expect("a model name is a package path")
}
