use std::collections::{BTreeMap, BTreeSet};

use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::gltf::model_bounds::ModelBounds;
use crate::zero_hour::animation_drop_cause::AnimationDropCause;
use crate::zero_hour::animation_import::{AnimationEnds, ConvertedAnimation};
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::dropped::Dropped;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::game_path;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::model_nodes::ModelNodes;

/// A model the objects name that the package lacks, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedModel {
    pub name: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkipReason {
    /// No file of the install holds it.
    Missing,
    /// It is a particle emitter, which the importer does not convert.
    Emitter,
    /// It is an aggregate of a model's HLOD, a render object the game adds on a pivot, which
    /// the importer does not convert.
    Aggregate,
}

/// What a model the objects name gives their looks: the nodes its looks read and the bounds its
/// looks' tops are measured from; nothing, as it draws nothing, as a `NULL` or a collision box;
/// or nothing, as the package lacks it, which the report names.
#[derive(Debug)]
pub(crate) enum WrittenModel {
    Nodes {
        nodes: ModelNodes,
        bounds: ModelBounds,
    },
    Nothing,
    Skipped,
}

/// What converts a model or an animation and writes its file: the assets it reads, the writer,
/// and what the models and the animations lacked so far.
#[derive(Debug)]
pub(super) struct Converter<'i> {
    pub(super) assets: ModelAssets<'i>,
    pub(super) skipped: Vec<SkippedModel>,
    pub(super) dropped_animations: Vec<Dropped<AnimationDropCause>>,
    /// Each model converted, by its file, which the import writes once it knows a look draws it.
    pub(super) models: BTreeMap<PackagePath, ConvertedModel>,
    /// Each animation converted, by its file, as `models` holds the models.
    pub(super) animations: BTreeMap<PackagePath, Vec<u8>>,
}

/// A model converted and not yet written: its file's bytes, the textures it draws with, and
/// the names of those no archive holds.
#[derive(Debug)]
pub(crate) struct ConvertedModel {
    pub(crate) glb: Vec<u8>,
    pub(crate) textures: BTreeSet<ArchivePath>,
    pub(crate) missing_textures: BTreeSet<String>,
}

impl Converter<'_> {
    /// Converts the animation `name` and keeps its file; its poses at its ends when it converted,
    /// and `dropped_animations` gaining one the package lacks.
    pub(super) fn animation(&mut self, name: &str) -> Result<Option<AnimationEnds>, ZeroHourError> {
        match ConvertedAnimation::of(name, &mut self.assets)? {
            ConvertedAnimation::Glb { glb, ends } => {
                self.animations
                    .insert(game_path::animation(name)?, glb.encode());
                Ok(Some(ends))
            }
            ConvertedAnimation::Dropped(cause) => {
                self.dropped_animations.push(Dropped {
                    name: name.to_owned(),
                    cause,
                });
                Ok(None)
            }
        }
    }

    /// Converts the model `name` and keeps its file; what it gives its looks, `skipped` gaining a
    /// model the package lacks.
    pub(super) fn convert(&mut self, name: &str) -> Result<WrittenModel, ZeroHourError> {
        let reason = match ModelImport::convert(name, &mut self.assets)? {
            Converted::Model(import) => {
                let bounds = ModelBounds::of(&import.glb.document);
                let converted = ConvertedModel {
                    glb: import.glb.encode(),
                    textures: import.textures,
                    missing_textures: import.missing_textures.into_iter().collect(),
                };
                self.models.insert(game_path::model(name)?, converted);
                self.skipped
                    .extend(import.aggregates.into_iter().map(|name| SkippedModel {
                        name,
                        reason: SkipReason::Aggregate,
                    }));
                return Ok(WrittenModel::Nodes {
                    nodes: import.nodes,
                    bounds,
                });
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
