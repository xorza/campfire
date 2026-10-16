use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use campfire_client_files::MaterialFile;
use campfire_codec::Toml;
use campfire_package_disk::PackageWriter;

use crate::zero_hour::error::{NoPackagePath, ZeroHourError};
use crate::zero_hour::game_path;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::model_parts::ModelParts;
use crate::zero_hour::{SkipReason, SkippedModel};

/// The models of an import: each converted once, at `client/models/<name>.glb`, with the material
/// files its shaders need, into the package's writer; the parts each one's looks read; the models
/// the objects name that the package lacks; and the textures the models name that no archive
/// holds.
#[derive(Debug)]
pub(crate) struct ModelWriter<'i, 'w> {
    converter: Converter<'i, 'w>,
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
struct Converter<'i, 'w> {
    assets: ModelAssets<'i>,
    writer: &'w mut PackageWriter,
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

impl<'i, 'w> ModelWriter<'i, 'w> {
    pub(crate) const fn new(assets: ModelAssets<'i>, writer: &'w mut PackageWriter) -> Self {
        ModelWriter {
            converter: Converter {
                assets,
                writer,
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

    pub(crate) fn report(self) -> ModelReport {
        ModelReport {
            skipped: self.converter.skipped,
            missing_textures: self.converter.missing_textures.into_iter().collect(),
        }
    }
}

impl Converter<'_, '_> {
    /// Converts the model `name` and writes its files; what it gives its looks, `skipped` gaining
    /// a model the package lacks.
    fn convert(&mut self, name: &str) -> Result<WrittenModel, ZeroHourError> {
        let reason = match ModelImport::convert(name, &mut self.assets)? {
            Converted::Model(import) => {
                self.writer
                    .write(game_path::model(name)?, &import.glb.encode())?;
                for material in &import.materials {
                    let text = Toml::write(&material.file);
                    let path = MaterialFile::path(&material.name).ok_or_else(|| {
                        NoPackagePath(format!("{}/{}.toml", MaterialFile::DIR, material.name))
                    })?;
                    self.writer.write(path, text.as_bytes())?;
                }
                self.missing_textures.extend(import.missing_textures);
                self.skipped
                    .extend(import.aggregates.into_iter().map(|name| SkippedModel {
                        name,
                        reason: SkipReason::Aggregate,
                    }));
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
