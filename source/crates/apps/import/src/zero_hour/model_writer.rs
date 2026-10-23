use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use campfire_client_files::{
    ClientModel, ClientUnit, ClientUnits, LookVariant, ModelScale, ModelTop,
};
use campfire_common::DeclaredName;
use campfire_package_disk::PackageWriter;

use crate::gltf::model_bounds::ModelBounds;
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::ini::object_ini::{
    DrawKind, DrawModule, DrawnState, ObjectIni, PlacedAs, StateLook,
};
use crate::zero_hour::format::look_conditions::LookConditions;
use crate::zero_hour::game_path;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::map_import;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::model_nodes::ModelNodes;

/// A model the objects name that the package lacks, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedModel {
    pub name: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// No file of the install holds it.
    Missing,
    /// It is a particle emitter, which the importer does not convert.
    Emitter,
    /// It is an aggregate of a model's HLOD, a render object the game adds on a pivot, which
    /// the importer does not convert.
    Aggregate,
}

/// The models of an import and the client's looks of the objects, of `client/units.toml`: each
/// model converted once, at `client/models/<name>.glb`, into the package's writer; the nodes each
/// one's looks read; the models the objects name that the package lacks; and the textures the
/// models name that no archive holds.
#[derive(Debug)]
pub(crate) struct ModelWriter<'i, 'w> {
    converter: Converter<'i, 'w>,
    written: BTreeMap<String, WrittenModel>,
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

/// What converts a model and writes its files: the assets it reads, the writer, and what the
/// models lacked so far.
#[derive(Debug)]
struct Converter<'i, 'w> {
    assets: ModelAssets<'i>,
    writer: &'w mut PackageWriter,
    skipped: Vec<SkippedModel>,
    missing_textures: BTreeSet<String>,
    textures: BTreeSet<ArchivePath>,
}

/// What the models of an import lacked and drew with: the models the package lacks, the textures
/// no archive holds, and the files of those they draw with.
#[derive(Debug)]
pub(crate) struct ModelReport {
    pub(crate) skipped: Vec<SkippedModel>,
    pub(crate) missing_textures: Vec<String>,
    pub(crate) textures: BTreeSet<ArchivePath>,
}

impl<'i, 'w> ModelWriter<'i, 'w> {
    pub(crate) const fn new(assets: ModelAssets<'i>, writer: &'w mut PackageWriter) -> Self {
        ModelWriter {
            converter: Converter {
                assets,
                writer,
                skipped: Vec::new(),
                missing_textures: BTreeSet::new(),
                textures: BTreeSet::new(),
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
            textures: self.converter.textures,
        }
    }
}

/// The models a look draws on a map of some conditions, and whether the package lacks a model
/// it names.
#[derive(Debug)]
struct LookModels {
    models: Vec<ClientModel>,
    skipped: bool,
}

impl ModelWriter<'_, '_> {
    /// The looks of `client/units.toml` of `ini`'s objects: each object's look, its models, each at
    /// its object's asset scale, with the nodes its default look hides and the node its turret
    /// turns (02-engine-core.md, The client draws a projectile with its look), a look of
    /// no models for an object whose draw modules name none, as it draws nothing, and no look for
    /// one whose models the package lacks, which the client shows as the engine's shape; and the
    /// look of each
    /// object that maps place as client objects, as the client's tree or prop buffer draws it
    /// (09-zero-hour.md, The multiplayer rule for client objects): a tree's models of its tree draw
    /// modules, a prop's of its first draw module, when the `W3DModelDraw` family's, each created
    /// by its name, so only the meshes hidden by their own flag are hidden.
    pub(crate) fn units<'r>(
        &mut self,
        ini: &'r ObjectIni,
        names: &mut ImportNames<'r>,
    ) -> Result<ClientUnits, ZeroHourError> {
        let mut units = ClientUnits::default();
        let shown = StateLook::default();
        for object in ini.objects() {
            let scale = object.scale();
            let draws = object.draws();
            // A unit draws each module's best state, as its drawable does.
            let unit = |conditions| draws.iter().map(move |draw| draw.drawn(conditions));
            let Some(mut unit_look) = self.look(unit, scale)? else {
                continue;
            };
            // A preview of its build stands at the angle the original's cursor shows it at.
            unit_look.placement_angle = object.build().placement_angle;
            unit_look.turret = self.turret(draws)?;
            let declared = DeclaredName::from(names.take(object)?);
            // A tree draws its module's model whatever the map; a prop the model of its first
            // module's best state, all of it shown, as `W3DTerrainVisual::addProp` draws it.
            let unhidden = |model| DrawnState {
                model,
                look: &shown,
            };
            let drawn = |conditions| match object.placed_as() {
                PlacedAs::Bridge | PlacedAs::Unit => Vec::new(),
                PlacedAs::Tree => draws
                    .iter()
                    .filter(|draw| draw.kind == DrawKind::Tree)
                    .map(|draw| unhidden(draw.model.as_deref()))
                    .collect(),
                PlacedAs::Prop => draws
                    .first()
                    .filter(|draw| draw.kind == DrawKind::Family)
                    .map(|draw| unhidden(draw.drawn(conditions).model))
                    .into_iter()
                    .collect(),
            };
            if let Some(object_look) = self.look(drawn, scale)?
                && !object_look.models.is_empty()
            {
                units.objects.insert(declared.clone(), object_look);
            }
            units.units.insert(declared, unit_look);
        }
        Ok(units)
    }

    /// The node the turret of `draws` turns: the pivot of the first module's model that its
    /// default state's `Turret` names, of either case, as `W3DModelDraw` finds the bone; none when
    /// no module names one its model has.
    fn turret(&mut self, draws: &[DrawModule]) -> Result<Option<String>, ZeroHourError> {
        for draw in draws {
            let (Some(model), Some(bone)) = (&draw.model, &draw.turret) else {
                continue;
            };
            if let WrittenModel::Nodes { nodes, .. } = self.model(model)?
                && let Some(pivot) = nodes.pivot_name(bone)
            {
                return Ok(Some(pivot.to_owned()));
            }
        }
        Ok(None)
    }

    /// The look of what `drawn` draws on a map of each conditions, at `scale`: its models on a
    /// map of none, and, when a night map, a snowy map or both draw other models, a variant for
    /// each of the three, so the client's best match finds each one's own. A look that draws
    /// nothing is empty when no model it names draws anything, and none when the package lacks
    /// one, so the client shows its loss.
    fn look<'d, I>(
        &mut self,
        drawn: impl Fn(LookConditions) -> I,
        scale: ModelScale,
    ) -> Result<Option<ClientUnit>, ZeroHourError>
    where
        I: IntoIterator<Item = DrawnState<'d>>,
    {
        let own = self.look_models(drawn(LookConditions::default()), scale)?;
        let mut skipped = own.skipped;
        let mut variants = Vec::new();
        for conditions in LookConditions::VARIANTS {
            let looked = self.look_models(drawn(conditions), scale)?;
            skipped |= looked.skipped;
            variants.push(LookVariant {
                conditions: map_import::condition_names(conditions),
                models: looked.models,
            });
        }
        if variants.iter().all(|variant| variant.models == own.models) {
            variants.clear();
        }
        if skipped && own.models.is_empty() && variants.is_empty() {
            return Ok(None);
        }
        Ok(Some(ClientUnit {
            models: own.models,
            variants,
            placement_angle: None,
            turret: None,
        }))
    }

    /// The models of `drawn`, each with the look whose hidden nodes it hides, at `scale`, of those
    /// that draw one the package holds, and whether the package lacks one it names.
    fn look_models<'d>(
        &mut self,
        drawn: impl IntoIterator<Item = DrawnState<'d>>,
        scale: ModelScale,
    ) -> Result<LookModels, ZeroHourError> {
        let mut looked = LookModels {
            models: Vec::new(),
            skipped: false,
        };
        for DrawnState { model, look } in drawn {
            let Some(model) = model else {
                continue;
            };
            match self.model(model)? {
                WrittenModel::Nodes { nodes, bounds } => {
                    let hide = nodes.hidden(look);
                    let top = bounds
                        .top(&hide)
                        .and_then(|top| ModelTop::new(top * scale.get()));
                    looked.models.push(ClientModel {
                        model: game_path::model(model)?,
                        scale,
                        hide,
                        top,
                    });
                }
                WrittenModel::Nothing => {}
                WrittenModel::Skipped => looked.skipped = true,
            }
        }
        Ok(looked)
    }
}

impl Converter<'_, '_> {
    /// Converts the model `name` and writes its files; what it gives its looks, `skipped` gaining
    /// a model the package lacks.
    fn convert(&mut self, name: &str) -> Result<WrittenModel, ZeroHourError> {
        let reason = match ModelImport::convert(name, &mut self.assets)? {
            Converted::Model(import) => {
                let bounds = ModelBounds::of(&import.glb.document);
                self.writer
                    .write(game_path::model(name)?, &import.glb.encode())?;
                self.missing_textures.extend(import.missing_textures);
                self.textures.extend(import.textures);
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
