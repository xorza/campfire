use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use campfire_client_files::{
    AnimationMode, ClientModel, ClientUnit, ClientUnits, ConditionTest, LookDraw, LookShown,
    LookState, LookTransition, LookVariant, LookVariation, ModelScale, ModelTop, NodeUpgrade,
    ObjectLook, SpeedRange, StartFrame, StateAnimation, StateClip, StatePlay,
};
use campfire_common::{DeclaredName, EnumAll};
use campfire_package_disk::PackageWriter;
use serde::{Deserialize, Serialize};

use crate::gltf::model_bounds::ModelBounds;
use crate::zero_hour::animation_import::{AnimationDropCause, ConvertedAnimation};
use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::dropped::Dropped;
use crate::zero_hour::error::{NameClash, NameError, ZeroHourError};
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::object_ini::ini_animation::{
    AnimationFlag, IniAnimation, IniMode,
};
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::UpgradeEffect;
use crate::zero_hour::format::ini::object_ini::{
    DrawKind, DrawModule, DrawnState, HideShow, IniObject, ObjectIni, PlacedAs, StateLook,
};
use crate::zero_hour::format::look_conditions::LookConditions;
use crate::zero_hour::game_path;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::look_flags::{FlagCondition, LookFlags};
use crate::zero_hour::map_import;
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::model_nodes::{ModelNodes, NodeChanges};
use crate::zero_hour::upgrade_import;

/// A model the objects name that the package lacks, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedModel {
    pub name: String,
    pub reason: SkipReason,
}

/// The condition states of an object's draw module, or of its whole look, the import does not
/// convert, by the object's name, why, and their count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedStates {
    pub object: String,
    pub cause: StateDropCause,
    pub states: usize,
}

/// Why the import drops a look's condition states: its draw module draws no model in any of them,
/// so the look holds no draw of it; or the package lacks the model a default state of its object
/// names, so the object has no look, which the client shows as the engine's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumAll, Serialize, Deserialize)]
pub enum StateDropCause {
    Undrawn,
    Missing,
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

/// The models and the animations of an import and the client's looks of the objects, of
/// `client/units.toml`: each model converted once, at `client/models/<name>.glb`, and each
/// animation a unit's look names once, at `client/animations/<name>.glb`, into the package's
/// writer; the nodes each model's looks read; the animations converted or dropped; the models the
/// objects name that the package lacks; and the textures the models name that no archive holds.
#[derive(Debug)]
pub(crate) struct ModelWriter<'i, 'w> {
    converter: Converter<'i, 'w>,
    written: BTreeMap<String, WrittenModel>,
    /// Each animation a written state names, whether it converted.
    animations: BTreeMap<String, bool>,
    conditions: BTreeMap<DeclaredName, ConditionTest>,
    dropped_states: Vec<DroppedStates>,
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
struct Converter<'i, 'w> {
    assets: ModelAssets<'i>,
    writer: &'w mut PackageWriter,
    skipped: Vec<SkippedModel>,
    dropped_animations: Vec<Dropped<AnimationDropCause>>,
    missing_textures: BTreeSet<String>,
    textures: BTreeSet<ArchivePath>,
}

/// What the models and the animations of an import lacked and drew with: the models the package
/// lacks, the animations it drops, the textures no archive holds, and the files of those the
/// models draw with.
#[derive(Debug)]
pub(crate) struct ModelReport {
    pub(crate) skipped: Vec<SkippedModel>,
    pub(crate) dropped_states: Vec<DroppedStates>,
    pub(crate) dropped_animations: Vec<Dropped<AnimationDropCause>>,
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
                dropped_animations: Vec::new(),
                missing_textures: BTreeSet::new(),
                textures: BTreeSet::new(),
            },
            written: BTreeMap::new(),
            animations: BTreeMap::new(),
            conditions: BTreeMap::new(),
            dropped_states: Vec::new(),
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
            dropped_states: self.dropped_states,
            dropped_animations: self.converter.dropped_animations,
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

/// An object's `SubObjectsUpgrade`: the tag its modifier gives, and the sub-objects it shows,
/// then those it hides, as `SubObjectsUpgrade::upgradeImplementation` changes them in order.
#[derive(Debug, Clone)]
struct SubObjectsUpgrade {
    tag: DeclaredName,
    changes: Vec<HideShow>,
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
    /// by its name, so only the meshes hidden by their own flag are hidden; and the look of each
    /// build variation of an object of them, its own, or its object's for a variation whose models
    /// the package lacks (02-engine-core.md, A look varies by its unit's variation).
    pub(crate) fn units<'r>(
        &mut self,
        ini: &'r ObjectIni,
        names: &mut ImportNames<'r>,
        flags: &LookFlags,
    ) -> Result<ClientUnits, ZeroHourError> {
        let mut units = ClientUnits::default();
        let shown = StateLook::default();
        for object in ini.objects() {
            let scale = object.scale();
            let draws = object.draws();
            let Some(own) = self.unit_draws(object, flags)? else {
                continue;
            };
            let mut unit_look = ClientUnit::new(own);
            // A preview of its build stands at the angle the original's cursor shows it at.
            unit_look.placement_angle = object.build().placement_angle;
            unit_look.turret = self.turret(draws)?;
            if let Some(of_parent) = ini.variations_of(object) {
                let mut looks = Vec::with_capacity(of_parent.len());
                for variation in of_parent {
                    looks.push(self.variation_look(variation, &unit_look, flags)?);
                }
                unit_look.variations = looks;
            }
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
            if let Some(object_look) = self.object_look(drawn, scale)?
                && !object_look.models.is_empty()
            {
                units.objects.insert(declared.clone(), object_look);
            }
            units.units.insert(declared, unit_look);
        }
        units.conditions = std::mem::take(&mut self.conditions);
        Ok(units)
    }

    /// The look of `variation`, a build variation of the object of `parent`: its draws and its
    /// turret's node, or `parent`'s own for one whose models the package lacks.
    fn variation_look(
        &mut self,
        variation: &IniObject,
        parent: &ClientUnit,
        flags: &LookFlags,
    ) -> Result<LookVariation, ZeroHourError> {
        let draws = variation.draws();
        let look = match self.unit_draws(variation, flags)? {
            Some(own) => LookVariation {
                draws: own,
                turret: self.turret(draws)?,
            },
            None => LookVariation {
                draws: parent.draws.clone(),
                turret: parent.turret.clone(),
            },
        };
        Ok(look)
    }

    /// The draws of a unit of `object`, at its scale, each module's states, its default first,
    /// and the flags it ignores, as each draw module of a drawable picks its best state, a map's
    /// conditions among the unit's (02-engine-core.md, A look is states of conditions): each
    /// state's sets of flags its conditions, whose tests `flags` gives, its models of those the
    /// package holds, and, of `ADJUST_HEIGHT_BY_CONSTRUCTION_PERCENT`, its rise, the object's
    /// height at its scale, as `W3DModelDraw::adjustTransformMtx` sinks its scaled model by
    /// `getMaxHeightAbovePosition` (04-capabilities/production.md, A building rises with its site). A module
    /// none of whose states draws a model is dropped. Draws of no model are empty when no model
    /// they name draws anything, and none when the package lacks one a default names, so the
    /// client shows its loss.
    fn unit_draws(
        &mut self,
        object: &IniObject,
        flags: &LookFlags,
    ) -> Result<Option<Vec<LookDraw>>, ZeroHourError> {
        let (draws, scale) = (object.draws(), object.scale());
        let height = object.geometry().height() * scale.get();
        let upgrades = SubObjectsUpgrade::of(object);
        let rise = |animation: &IniAnimation| {
            let rises = animation.flags.holds(AnimationFlag::AdjustHeight);
            let rise = rises.then(|| game_float::to_num(height));
            rise.transpose().map_err(|_| ZeroHourError::StateNumber)
        };
        // A look of no default model the package holds, but one it lacks, is none: it converts
        // none of its states' animations.
        let mut skipped = false;
        let mut drawn = false;
        for draw in draws {
            let default = draw
                .states()
                .next()
                .expect("a module has its default state");
            let looked = self.look_models([default.drawn], scale, &upgrades)?;
            skipped |= looked.skipped;
            drawn |= !looked.models.is_empty();
        }
        if skipped && !drawn {
            let states = draws.iter().map(|draw| draw.states().count()).sum();
            self.drop_states(object, StateDropCause::Missing, states);
            return Ok(None);
        }
        let mut look_draws = Vec::new();
        for draw in draws {
            let mut keys = DrawKeys::default();
            let mut states = Vec::new();
            for state in draw.states() {
                let looked = self.look_models([state.drawn], scale, &upgrades)?;
                let mut sets = Vec::with_capacity(state.sets.len());
                for set in state.sets {
                    let mut names = Vec::with_capacity(set.flags().len());
                    for flag in set.flags() {
                        names.push(self.condition(flag, flags)?);
                    }
                    sets.push(names);
                }
                let animation = state.animation;
                states.push(LookState {
                    sets,
                    models: looked.models,
                    animation: self.state_animation(animation)?,
                    rise: rise(animation)?,
                    key: animation
                        .key
                        .as_deref()
                        .map(|key| keys.key(key))
                        .transpose()?,
                    waits_for: (animation.waits_for.as_deref())
                        .map(|key| keys.key(key))
                        .transpose()?,
                });
            }
            if states.iter().all(|state| state.models.is_empty()) {
                self.drop_states(object, StateDropCause::Undrawn, states.len());
                continue;
            }
            let mut ignore = Vec::with_capacity(draw.ignored().len());
            for flag in draw.ignored() {
                ignore.push(self.condition(flag, flags)?);
            }
            let mut transitions = Vec::with_capacity(draw.transitions().len());
            for transition in draw.transitions() {
                let looked = self.look_models([transition.drawn()], scale, &upgrades)?;
                let shown = LookShown {
                    models: looked.models,
                    animation: self.state_animation(&transition.animation)?,
                    rise: rise(&transition.animation)?,
                };
                let (from, to) = (keys.key(&transition.from)?, keys.key(&transition.to)?);
                transitions.push(
                    LookTransition::new(from, to, shown)
                        .expect("a transition the reader took plays once between two keys"),
                );
            }
            look_draws.push(
                LookDraw::new(states, ignore, transitions)
                    .expect("a module's states each of a set, its transitions of a pair once"),
            );
        }
        Ok(Some(look_draws))
    }

    /// The animation of a state of `animation`, of each clip that converts, as `W3DModelDraw`
    /// plays it (02-engine-core.md, A state plays its animation by its mode); none for a state of
    /// no clip that converts. Its start is the first of its flags `adjustAnimation` reads, a
    /// random frame, the first, then the last.
    fn state_animation(
        &mut self,
        animation: &IniAnimation,
    ) -> Result<Option<StateAnimation>, ZeroHourError> {
        let number = |value: f32| game_float::to_num(value).map_err(|_| ZeroHourError::StateNumber);
        let mut clips = Vec::with_capacity(animation.clips.len());
        for clip in &animation.clips {
            if !self.animation(&clip.name)? {
                continue;
            }
            let stride = (clip.distance > 0.0).then(|| number(clip.distance));
            clips.push(StateClip {
                animation: game_path::animation(&clip.name)?,
                stride: stride.transpose()?,
            });
        }
        if clips.is_empty() {
            return Ok(None);
        }
        let flags = animation.flags;
        let start = if flags.holds(AnimationFlag::RandomStart) {
            StartFrame::Random
        } else if flags.holds(AnimationFlag::StartFrameFirst) {
            StartFrame::First
        } else if flags.holds(AnimationFlag::StartFrameLast) {
            StartFrame::Last
        } else {
            StartFrame::Natural
        };
        let maintain = [
            AnimationFlag::MaintainFrame,
            AnimationFlag::MaintainFrame2,
            AnimationFlag::MaintainFrame3,
            AnimationFlag::MaintainFrame4,
        ]
        .iter()
        .enumerate()
        .filter(|&(_, &flag)| flags.holds(flag))
        .fold(0, |groups, (bit, _)| groups | (1 << bit));
        let [least, most] = animation.speed;
        let speed =
            SpeedRange::new(number(least)?, number(most)?).ok_or(ZeroHourError::StateNumber)?;
        let play = StatePlay {
            idle: animation.idle(),
            start,
            maintain,
            restart: flags.holds(AnimationFlag::RestartWhenComplete),
            speed,
        };
        let mode = match animation.mode {
            IniMode::Manual => AnimationMode::Manual,
            IniMode::Loop => AnimationMode::Loop,
            IniMode::Once => AnimationMode::Once,
            IniMode::PingPong => AnimationMode::PingPong,
            IniMode::LoopBackwards => AnimationMode::LoopBackwards,
            IniMode::OnceBackwards => AnimationMode::OnceBackwards,
        };
        Ok(StateAnimation::new(clips, mode, play))
    }

    /// Whether the animation `name` converts, which it converts and writes the first time a
    /// written state names it.
    fn animation(&mut self, name: &str) -> Result<bool, ZeroHourError> {
        if let Some(&converted) = self.animations.get(name) {
            return Ok(converted);
        }
        let converted = self.converter.animation(name)?;
        self.animations.insert(name.to_owned(), converted);
        Ok(converted)
    }

    /// The condition of `flag`, whose test `flags` gives, written to the conditions once.
    /// Reports `states` of `object` the import drops for `cause`.
    fn drop_states(&mut self, object: &IniObject, cause: StateDropCause, states: usize) {
        self.dropped_states.push(DroppedStates {
            object: object.name.clone(),
            cause,
            states,
        });
    }

    fn condition(&mut self, flag: &str, flags: &LookFlags) -> Result<DeclaredName, ZeroHourError> {
        let FlagCondition { name, test } = flags.condition(flag)?;
        self.conditions.entry(name.clone()).or_insert(test);
        Ok(name)
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

    /// The look of a client object of what `drawn` draws on a map of each conditions, at `scale`:
    /// its models on a map of none, and, when a night map, a snowy map or both draw other models,
    /// a variant for each of the three, so the client's best match finds each one's own. A look
    /// that draws nothing is empty when no model it names draws anything, and none when the
    /// package lacks one.
    fn object_look<'d, I>(
        &mut self,
        drawn: impl Fn(LookConditions) -> I,
        scale: ModelScale,
    ) -> Result<Option<ObjectLook>, ZeroHourError>
    where
        I: IntoIterator<Item = DrawnState<'d>>,
    {
        let own = self.look_models(drawn(LookConditions::default()), scale, &[])?;
        let mut skipped = own.skipped;
        let mut variants = Vec::new();
        for conditions in LookConditions::VARIANTS {
            let looked = self.look_models(drawn(conditions), scale, &[])?;
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
        Ok(Some(ObjectLook {
            variants,
            ..ObjectLook::new(own.models)
        }))
    }

    /// The models of `drawn`, each with the look whose hidden nodes it hides, at `scale`, of those
    /// that draw one the package holds, and whether the package lacks one it names.
    fn look_models<'d>(
        &mut self,
        drawn: impl IntoIterator<Item = DrawnState<'d>>,
        scale: ModelScale,
        upgrades: &[SubObjectsUpgrade],
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
                    let mut node_upgrades = Vec::new();
                    for SubObjectsUpgrade { tag, changes } in upgrades {
                        let NodeChanges { show, hide } = nodes.changes(changes);
                        if show.is_empty() && hide.is_empty() {
                            continue;
                        }
                        node_upgrades.push(NodeUpgrade {
                            condition: tag.clone(),
                            show,
                            hide,
                        });
                    }
                    looked.models.push(ClientModel {
                        model: game_path::model(model)?,
                        scale,
                        hide,
                        top,
                        upgrades: node_upgrades,
                    });
                }
                WrittenModel::Nothing => {}
                WrittenModel::Skipped => looked.skipped = true,
            }
        }
        let upgrades = looked.models.iter().flat_map(|model| &model.upgrades);
        for NodeUpgrade { condition, .. } in upgrades {
            let test = ConditionTest::Tag {
                tag: condition.clone(),
            };
            self.conditions.entry(condition.clone()).or_insert(test);
        }
        Ok(looked)
    }
}

impl SubObjectsUpgrade {
    /// The `SubObjectsUpgrade`s of `object` whose tag has a name, as the upgrades' import keeps
    /// them (09-zero-hour.md, An upgrade's look is a tag).
    fn of(object: &IniObject) -> Vec<Self> {
        let mut upgrades = Vec::new();
        for module in object.upgrades() {
            let UpgradeEffect::SubObjects { show, hide } = &module.effect else {
                continue;
            };
            let Ok(Some(tag)) = upgrade_import::look_tag(module) else {
                continue;
            };
            let change = |hide| {
                move |name: &String| HideShow {
                    name: name.clone(),
                    hide,
                }
            };
            let changes = show.iter().map(change(false));
            let changes = changes.chain(hide.iter().map(change(true))).collect();
            upgrades.push(Self { tag, changes });
        }
        upgrades
    }
}

/// The transition keys a draw module named, by the names the package gives them, each with the
/// key that took it, as `NAMEKEY` tells keys apart by their text.
#[derive(Debug, Default)]
struct DrawKeys {
    named: BTreeMap<DeclaredName, String>,
}

impl DrawKeys {
    /// The name the package gives the key `key`, as `ImportName` gives one; an error past the
    /// bytes a name holds, or when another key of the module gave that name.
    fn key(&mut self, key: &str) -> Result<DeclaredName, NameError> {
        let name = ImportName::latin(key).declared()?;
        match self.named.entry(name.clone()) {
            Entry::Occupied(taken) if taken.get() != key => Err(NameError::Clash(NameClash {
                first: taken.get().clone(),
                second: key.to_owned(),
            })),
            Entry::Occupied(_) => Ok(name),
            Entry::Vacant(free) => {
                free.insert(key.to_owned());
                Ok(name)
            }
        }
    }
}

impl Converter<'_, '_> {
    /// Converts the animation `name` and writes its file; whether it converted, and
    /// `dropped_animations` gaining one the package lacks.
    fn animation(&mut self, name: &str) -> Result<bool, ZeroHourError> {
        match ConvertedAnimation::of(name, &mut self.assets)? {
            ConvertedAnimation::Glb(glb) => {
                self.writer
                    .write(game_path::animation(name)?, &glb.encode())?;
                Ok(true)
            }
            ConvertedAnimation::Dropped(cause) => {
                self.dropped_animations.push(Dropped {
                    name: name.to_owned(),
                    cause,
                });
                Ok(false)
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_draws_keys_take_the_packages_names_and_two_of_one_name_clash() {
        // `lit-up` gives `lit_up`, again as often as it is named; `2nd` starts with no letter, so
        // `x2nd`; `lit_up` gives the name `lit-up` took, so it clashes.
        let mut keys = DrawKeys::default();
        let name = |text| DeclaredName::new(text).unwrap();
        assert_eq!(keys.key("lit-up"), Ok(name("lit_up")));
        assert_eq!(keys.key("lit-up"), Ok(name("lit_up")));
        assert_eq!(keys.key("2nd"), Ok(name("x2nd")));
        let clash = NameClash {
            first: "lit-up".to_owned(),
            second: "lit_up".to_owned(),
        };
        assert_eq!(keys.key("lit_up"), Err(NameError::Clash(clash)));
    }
}
