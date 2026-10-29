use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use bevy_math::Vec3;
use campfire_client_files::{
    AnimationMode, ClientModel, ClientUnit, ClipRounds, ConditionTest, FireBone, LookDraw,
    LookShown, LookState, LookTransition, LookVariant, LookVariation, ModelScale, ModelTop,
    NodeUpgrade, ObjectLook, SpeedRange, StartFrame, StateAnimation, StateClip, StatePlay,
};
use campfire_combat::{BarrelData, UnitOffset};
use campfire_common::DeclaredName;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

use crate::pose::RigidPose;
use crate::zero_hour::additions::Additions;
use crate::zero_hour::airfield_bones::{AirfieldBones, PlaceBones};
use crate::zero_hour::animation_drop_cause::AnimationDropCause;
use crate::zero_hour::animation_import::AnimationEnds;
use crate::zero_hour::barrel_recoil::BarrelRecoil;
use crate::zero_hour::carried_look::CarriedLook;
use crate::zero_hour::draw_sets::DrawSets;
use crate::zero_hour::dropped::Dropped;
use crate::zero_hour::error::{ModelError, NameClash, NameError, ZeroHourError};
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::object_ini::draw_module::{
    DrawKind, DrawModule, DrawnState, HideShow, StateLook,
};
use crate::zero_hour::format::ini::object_ini::ini_animation::{
    AnimationFlag, IniAnimation, IniMode,
};
use crate::zero_hour::format::ini::object_ini::ini_build::IniParking;
use crate::zero_hour::format::ini::object_ini::ini_contain::ContainClass;
use crate::zero_hour::format::ini::object_ini::ini_object::{IniObject, PlacedAs};
use crate::zero_hour::format::ini::object_ini::ini_recoil::IniRecoil;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::UpgradeEffect;
use crate::zero_hour::format::look_conditions::LookConditions;
use crate::zero_hour::game_path;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::look_flags::{FlagCondition, LookFlags};
use crate::zero_hour::model_assets::ModelAssets;
use crate::zero_hour::model_bones::ModelBones;
use crate::zero_hour::model_converter::ConvertedModel;
use crate::zero_hour::model_converter::Converter;
use crate::zero_hour::model_converter::SkippedModel;
use crate::zero_hour::model_converter::WrittenModel;
use crate::zero_hour::model_nodes::NodeChanges;
use crate::zero_hour::portable_structure;
use crate::zero_hour::production_bones::ProductionBones;
use crate::zero_hour::state_drop_cause::StateDropCause;
use crate::zero_hour::upgrade_import;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The condition states of an object's draw module, or of its whole look, the import does not
/// convert, by the object's name, why, and their count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedStates {
    pub object: String,
    pub cause: StateDropCause,
    pub states: usize,
}

/// The models and the animations of an import and the client's looks of the objects, of
/// `client/units.toml`: each model converted once, at `client/models/<name>.glb`, and each
/// animation a unit's look names once, at `client/animations/<name>.glb`, into the package's
/// writer; the nodes each model's looks read; the animations converted or dropped; the models the
/// objects name that the package lacks; and the textures the models name that no archive holds.
#[derive(Debug)]
pub(crate) struct ModelWriter<'i> {
    converter: Converter<'i>,
    written: BTreeMap<String, WrittenModel>,
    /// Each animation a written state names, its poses at its ends when it converted.
    animations: BTreeMap<String, Option<AnimationEnds>>,
    conditions: BTreeMap<DeclaredName, ConditionTest>,
    dropped_states: Vec<DroppedStates>,
}

/// The looks of an import's `client/units.toml`, which later stages complete before the import
/// writes them, the width of the track each look's treads lay, of the looks whose model names its
/// treads' bones, the points production reads of each model, and what each type reads of its
/// model, as its first additions.
#[derive(Debug)]
pub(crate) struct ImportedLooks {
    pub(crate) units: BTreeMap<DeclaredName, ClientUnit>,
    pub(crate) conditions: BTreeMap<DeclaredName, ConditionTest>,
    pub(crate) objects: BTreeMap<DeclaredName, ObjectLook>,
    pub(crate) track_widths: BTreeMap<DeclaredName, f32>,
    pub(crate) points: ProductionBones,
    /// The first additions of the types: each one's bones, and a garrison's fire points.
    pub(crate) additions: Additions,
}

/// What a unit reads of its model at rest: its turret's and weapons' bones, the width of the
/// track its treads lay, when its model names their bones, its helipad, its `HeliPark01` bone's
/// point on the ground, the game's axes, when its model names it, an airfield's places and
/// runways, and its dock, its `DockAction` bone's point, when its model names it.
#[derive(Debug)]
struct ModelRest {
    bones: ModelBones,
    track_width: Option<f32>,
    helipad: Option<[f32; 2]>,
    airfield: Option<AirfieldBones>,
    dock: Option<[f32; 2]>,
}

/// The bone an airfield makes a helicopter at (`ParkingPlaceBehavior::exitObjectViaDoor`).
const HELIPAD: &str = "HeliPark01";

/// The bone a gatherer docks at (`DockUpdate`'s `DockAction`).
const DOCK_ACTION: &str = "DockAction";

/// The most fire points a garrison holds, `GarrisonContain`'s `MAX_GARRISON_POINTS`.
const MOST_FIRE_POINTS: usize = 40;

/// The width of one tread, which the game adds to the span of a model's tread bones for its
/// track's width (`W3DTerrainTracks.cpp`, `DEFAULT_TRACK_WIDTH`).
const TREAD_WIDTH: f32 = 4.0;

/// What the models and the animations of an import lacked and converted: the models the package
/// lacks, the animations it drops, and each model and animation converted, by its file, which the
/// import writes once a look draws it.
#[derive(Debug)]
pub(crate) struct ModelReport {
    pub(crate) skipped: Vec<SkippedModel>,
    pub(crate) dropped_states: Vec<DroppedStates>,
    pub(crate) dropped_animations: Vec<Dropped<AnimationDropCause>>,
    pub(crate) models: BTreeMap<PackagePath, ConvertedModel>,
    pub(crate) animations: BTreeMap<PackagePath, Vec<u8>>,
}

impl<'i> ModelWriter<'i> {
    pub(crate) const fn new(assets: ModelAssets<'i>) -> Self {
        ModelWriter {
            converter: Converter {
                assets,
                skipped: Vec::new(),
                dropped_animations: Vec::new(),
                models: BTreeMap::new(),
                animations: BTreeMap::new(),
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
            models: self.converter.models,
            animations: self.converter.animations,
        }
    }
}

/// What the models of a draw module's states take beside their states: their object's scale,
/// the nodes each of its upgrades shows and hides, the weapon slots whose clips' rounds they show,
/// and the module's recoil, which times its barrels' flashes.
#[derive(Debug, Clone, Copy)]
struct ModuleModels<'a> {
    scale: ModelScale,
    upgrades: &'a [SubObjectsUpgrade],
    rounds_shown: [bool; 3],
    recoil: BarrelRecoil,
}

/// The models a look draws on a map of some conditions, whether the package lacks a model it
/// names, and the bones its weapon slots' fire effects play at.
#[derive(Debug)]
struct LookModels {
    models: Vec<ClientModel>,
    skipped: bool,
    fire_bones: Vec<FireBone>,
}

/// An object's `SubObjectsUpgrade`: the tag its modifier gives, and the sub-objects it shows,
/// then those it hides, as `SubObjectsUpgrade::upgradeImplementation` changes them in order.
#[derive(Debug, Clone)]
struct SubObjectsUpgrade {
    tag: DeclaredName,
    changes: Vec<HideShow>,
}

impl ModelWriter<'_> {
    /// The looks of `client/units.toml` of `ini`'s objects: each object's look, its models, each at
    /// its object's asset scale, with the nodes its default look hides and the node its turret
    /// turns (02-engine-core.md, The client draws a projectile with its look), a look of no models
    /// for an object whose draw modules name none, as it draws nothing, and no look for one whose
    /// models the package lacks, which the client shows as the engine's shape; and the look of each
    /// object that maps place as client objects, as the client's tree or prop buffer draws it
    /// (09-zero-hour.md, The multiplayer rule for client objects): a tree's models of its tree draw
    /// modules, a prop's of its first draw module, when the `W3DModelDraw` family's, each created
    /// by its name, so only the meshes hidden by their own flag are hidden; and the look of each
    /// build variation of an object of them, its own, or its object's for a variation whose models
    /// the package lacks (02-engine-core.md, A look varies by its unit's variation).
    pub(crate) fn units<'r>(
        &mut self,
        rules: &'r ZeroHourRules,
        names: &mut ImportNames<'r>,
        flags: &LookFlags,
    ) -> Result<ImportedLooks, ZeroHourError> {
        let ini = &rules.objects;
        let mut units = BTreeMap::new();
        let mut objects = BTreeMap::new();
        let mut additions = Additions::default();
        let mut track_widths = BTreeMap::new();
        let mut points = ProductionBones::default();
        let shown = StateLook::default();
        for object in ini.objects() {
            let scale = object.scale();
            let draws = object.draws();
            let own = match self.unit_draws(object, flags) {
                Ok(own) => own,
                // A number of one object's states the engine cannot hold drops its look alone
                // (09-zero-hour.md, A fault of one object drops its part).
                Err(ZeroHourError::StateNumber) => {
                    let states = object
                        .draws()
                        .iter()
                        .map(|draw| draw.states().count())
                        .sum();
                    self.drop_states(object, StateDropCause::Number, states);
                    None
                }
                Err(error) => return Err(error),
            };
            let Some(mut own) = own else {
                continue;
            };
            let carried = self.carried_models(rules, object)?;
            if let Some(first) = own.first_mut() {
                for (model, _) in &carried {
                    first.add_model(model);
                }
            }
            let mut unit_look = ClientUnit::new(own);
            // A preview of its build stands at the angle the original's cursor shows it at.
            unit_look.placement_angle = object.build().placement_angle;
            unit_look.turret = self.node(draws, |draw| draw.turret.as_deref())?;
            unit_look.turret_pitch = self.node(draws, |draw| draw.turret_pitch.as_deref())?;
            // A type of build variations is its first variation's, its bones too.
            let boned = rules.variations.fielded(ini, object);
            let ModelRest {
                bones: of_model,
                track_width,
                helipad,
                airfield,
                dock,
            } = self.bones(boned.draws(), boned.scale(), object.build().parking)?;
            let garrisoned = boned
                .contain()
                .filter(|contain| contain.class == ContainClass::Garrison && contain.enclosing)
                .map(|_| self.fire_points(boned.draws(), boned.scale()))
                .transpose()?
                .filter(|points| !points.is_empty());
            if let Some(of_parent) = rules.variations.objects_of(ini, object) {
                let mut looks = Vec::with_capacity(of_parent.len());
                for variation in of_parent {
                    looks.push(self.variation_look(variation, &unit_look, flags)?);
                }
                unit_look.variations = looks;
            }
            let type_name = names.take(object)?;
            let declared = type_name.declared().clone();
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
                objects.insert(declared.clone(), object_look);
            }
            if of_model != ModelBones::default() {
                additions.type_mut(&type_name).bones = Some(of_model);
            }
            if let Some(width) = track_width {
                track_widths.insert(declared.clone(), width);
            }
            if let Some(helipad) = helipad {
                points.helipads.insert(declared.clone(), helipad);
            }
            if let Some(airfield) = airfield {
                points.airfields.insert(declared.clone(), airfield);
            }
            if let Some(dock) = dock {
                points.docks.insert(declared.clone(), dock);
            }
            if let Some(points) = garrisoned {
                additions.type_mut(&type_name).fire_points = points;
            }
            if !carried.is_empty() {
                additions.type_mut(&type_name).carried =
                    carried.into_iter().map(|(_, look)| look).collect();
            }
            units.insert(declared, unit_look);
        }
        Ok(ImportedLooks {
            units,
            conditions: std::mem::take(&mut self.conditions),
            objects,
            track_widths,
            points,
            additions,
        })
    }

    /// The models of the portable structures the upgrades of `object` make inside it, of `rules`,
    /// each the model of the default state of a structure's draw of a bone of its container
    /// (`AttachToBoneInContainer`), drawn at that bone of its carrier's model, at the structure's
    /// scale, hidden from its root down but while its carrier holds its upgrade's tag, as the
    /// structure stands on its carrier while it lives, and hiding what that state hides, its
    /// barrels' flashes among it (riders-and-upgrades.md, A model may be drawn at another's bone);
    /// each with what of its look waits for the slots its structure's weapons take
    /// (riders-and-upgrades.md, A carried model's barrels are its carrier's). None of a model the
    /// package lacks.
    fn carried_models(
        &mut self,
        rules: &ZeroHourRules,
        object: &IniObject,
    ) -> Result<Vec<(ClientModel, CarriedLook)>, ZeroHourError> {
        let mut models = Vec::new();
        for module in object.upgrades() {
            let Some(structure) = portable_structure::structure(rules, module) else {
                continue;
            };
            let Some(tag) = portable_structure::tag(module)? else {
                continue;
            };
            let draws = structure.draws();
            let Some(draw) = draws.iter().find(|draw| draw.container_bone.is_some()) else {
                continue;
            };
            let default = draw
                .states()
                .next()
                .expect("a module has its default state");
            let Some(model) = default.drawn.model else {
                continue;
            };
            let WrittenModel::Nodes { nodes, .. } = self.model(model)? else {
                continue;
            };
            let Some(root) = nodes.pivots.first().map(|pivot| pivot.name.clone()) else {
                continue;
            };
            let module = ModuleModels {
                scale: structure.scale(),
                upgrades: &[],
                rounds_shown: draw.rounds_shown,
                recoil: BarrelRecoil::of(draw.recoil),
            };
            let LookModels {
                models: looked,
                fire_bones,
                ..
            } = self.look_models([default.drawn], module)?;
            let [mut carried] = <[ClientModel; 1]>::try_from(looked)
                .expect("a state's one model the package holds");
            let test = ConditionTest::Tag { tag: tag.clone() };
            self.conditions.entry(tag.clone()).or_insert(test);
            carried.hide.retain(|node| *node != root);
            carried.hide.insert(0, root.clone());
            carried.top = None;
            carried.upgrades = vec![NodeUpgrade {
                condition: tag,
                show: vec![root],
                hide: Vec::new(),
            }];
            carried.bone.clone_from(&draw.container_bone);
            let look = CarriedLook::new(
                ImportName::latin(&structure.name).declared()?,
                carried.model.clone(),
                fire_bones,
                std::mem::take(&mut carried.rounds),
            );
            models.push((carried, look));
        }
        Ok(models)
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
                turret: self.node(draws, |draw| draw.turret.as_deref())?,
                turret_pitch: self.node(draws, |draw| draw.turret_pitch.as_deref())?,
            },
            None => LookVariation {
                draws: parent.draws.clone(),
                turret: parent.turret.clone(),
                turret_pitch: parent.turret_pitch.clone(),
            },
        };
        Ok(look)
    }

    /// The draws of a unit of `object`, at its scale, each module's states, its default first, and
    /// the flags it ignores, as each draw module of a drawable picks its best state, a map's
    /// conditions among the unit's (02-engine-core.md, A look is states of conditions): each
    /// state's sets of flags its conditions, whose tests `flags` gives, its models of those the
    /// package holds, and, of `ADJUST_HEIGHT_BY_CONSTRUCTION_PERCENT`, its rise, the object's
    /// height at its scale, as `W3DModelDraw::adjustTransformMtx` sinks its scaled model by
    /// `getMaxHeightAbovePosition` (04-capabilities/production.md, A building rises with its site).
    /// A module none of whose states draws a model is dropped. Draws of no model are empty when no
    /// model they name draws anything, and none when the package lacks one a default names, so the
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
            let module = ModuleModels {
                scale,
                upgrades: &upgrades,
                rounds_shown: draw.rounds_shown,
                recoil: BarrelRecoil::of(draw.recoil),
            };
            let looked = self.look_models([default.drawn], module)?;
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
            let module = ModuleModels {
                scale,
                upgrades: &upgrades,
                rounds_shown: draw.rounds_shown,
                recoil: BarrelRecoil::of(draw.recoil),
            };
            // A state of no set left is never drawn, so its models and its animation convert not,
            // nor a wait for its key or a transition of it.
            let DrawSets {
                states: state_sets,
                tests,
            } = DrawSets::of(draw, flags)?;
            let mut keys = DrawKeys::default();
            let mut kept = Vec::new();
            for (state, sets) in draw.states().zip(state_sets) {
                if !sets.is_empty() {
                    let key = state.animation.key.as_deref().map(|key| keys.key(key));
                    kept.push((state, sets, key.transpose()?));
                }
            }
            let kept_keys: Vec<DeclaredName> =
                kept.iter().filter_map(|(_, _, key)| key.clone()).collect();
            let mut states = Vec::with_capacity(kept.len());
            for (state, sets, key) in kept {
                let looked = self.look_models([state.drawn], module)?;
                let animation = state.animation;
                let waits_for = (animation.waits_for.as_deref())
                    .map(|key| keys.key(key))
                    .transpose()?
                    .filter(|key| kept_keys.contains(key));
                states.push(LookState {
                    sets,
                    models: looked.models,
                    animation: self.state_animation(animation)?,
                    rise: rise(animation)?,
                    key,
                    waits_for,
                    fire_bones: looked.fire_bones,
                });
            }
            if states.iter().all(|state| state.models.is_empty()) {
                self.drop_states(object, StateDropCause::Undrawn, states.len());
                continue;
            }
            for (name, test) in tests {
                self.conditions.entry(name).or_insert(test);
            }
            // A flag no test says, which no unit holds, needs no ignoring.
            let mut ignore = Vec::with_capacity(draw.ignored().len());
            for flag in draw.ignored() {
                ignore.extend(self.condition(flag, flags)?);
            }
            let mut transitions = Vec::with_capacity(draw.transitions().len());
            for transition in draw.transitions() {
                let (from, to) = (keys.key(&transition.from)?, keys.key(&transition.to)?);
                if !(kept_keys.contains(&from) && kept_keys.contains(&to)) {
                    continue;
                }
                let looked = self.look_models([transition.drawn()], module)?;
                let shown = LookShown {
                    models: looked.models,
                    animation: self.state_animation(&transition.animation)?,
                    rise: rise(&transition.animation)?,
                };
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
    pub(super) fn animation(&mut self, name: &str) -> Result<bool, ZeroHourError> {
        Ok(self.animation_ends(name)?.is_some())
    }

    /// The poses at its ends of the animation `name`, which it converts and writes the first time
    /// a written state names it; none when it does not convert.
    fn animation_ends(&mut self, name: &str) -> Result<Option<&AnimationEnds>, ZeroHourError> {
        let ends = match self.animations.entry(name.to_owned()) {
            Entry::Occupied(known) => known.into_mut(),
            Entry::Vacant(new) => new.insert(self.converter.animation(name)?),
        };
        Ok(ends.as_ref())
    }

    /// Reports `states` of `object` the import drops for `cause`.
    fn drop_states(&mut self, object: &IniObject, cause: StateDropCause, states: usize) {
        self.dropped_states.push(DroppedStates {
            object: object.name.clone(),
            cause,
            states,
        });
    }

    /// The condition of `flag`, whose test `flags` gives, written to the conditions once; none for
    /// one no test says.
    fn condition(
        &mut self,
        flag: &str,
        flags: &LookFlags,
    ) -> Result<Option<DeclaredName>, ZeroHourError> {
        let FlagCondition { name, test } = flags.condition(flag)?;
        let Some(test) = test else {
            return Ok(None);
        };
        self.conditions.entry(name.clone()).or_insert(test);
        Ok(Some(name))
    }

    /// The node of `draws` that `bone` names, as its turret turns or pitches it: the pivot of the
    /// first module's model that its default state's `Turret` or `TurretPitch` names, of either
    /// case, as `W3DModelDraw` finds the bone; none when no module names one its model has.
    fn node(
        &mut self,
        draws: &[DrawModule],
        bone: fn(&DrawModule) -> Option<&str>,
    ) -> Result<Option<String>, ZeroHourError> {
        for draw in draws {
            let (Some(model), Some(bone)) = (&draw.model, bone(draw)) else {
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

    /// What a unit of `draws`, at `scale`, reads of its model: of its first module of the
    /// `W3DModelDraw` family, its default state's model as its first animation places it, its
    /// turret's and its pitch's pivots, each weapon slot's barrels of its launch bones, and its
    /// track's width, the span of its `TREADFX01` and `TREADFX02` bones and one tread's, as
    /// `computeTrackSpacing` measures it; none of a model the package lacks. An error for a bone
    /// past the engine's bound.
    fn bones(
        &mut self,
        draws: &[DrawModule],
        scale: ModelScale,
        parking: Option<IniParking>,
    ) -> Result<ModelRest, ZeroHourError> {
        let none = ModelRest {
            bones: ModelBones::default(),
            track_width: None,
            helipad: None,
            airfield: None,
            dock: None,
        };
        let drawn = draws.iter().find(|draw| draw.kind == DrawKind::Family);
        let Some((draw, model)) = drawn.and_then(|draw| Some((draw, draw.model.as_ref()?))) else {
            return Ok(none);
        };
        let default = draw
            .states()
            .next()
            .expect("a module has its default state");
        let animated = self.rested(default.animation)?;
        let WrittenModel::Nodes { nodes, .. } = self.model(model)? else {
            return Ok(none);
        };
        let pristine = nodes.pristine(animated.as_deref());
        let beyond = || ZeroHourError::Model {
            model: model.clone(),
            error: ModelError::Bound,
        };
        let number = |value: f32| game_float::to_num(value).map_err(|_| beyond());
        let point = |at: Vec3| {
            let [x, y, z] = (at * scale.get()).to_array().map(number);
            UnitOffset::new([x?, y?, z?]).ok_or_else(beyond)
        };
        let pivot = |bone: Option<&String>| {
            let at = bone.and_then(|bone| nodes.pivot_point(bone, &pristine));
            at.map(point).transpose()
        };
        let mut barrels: [Vec<BarrelData>; 3] = Default::default();
        for (of_slot, weapon) in barrels.iter_mut().zip(&draw.look.weapons) {
            for barrel in nodes.launches(weapon, &pristine) {
                let [x, y, z] = barrel.way.to_array().map(number);
                let data = BarrelData::new(point(barrel.at)?, [x?, y?, z?]);
                of_slot.push(data.ok_or_else(beyond)?);
            }
        }
        let track_width =
            (nodes.tread_span(&pristine)).map(|span| span * scale.get() + TREAD_WIDTH);
        // A pivot's point is on the engine's axes, its `z` the game's `−y`.
        let ground = |at: Vec3| [at.x * scale.get(), -at.z * scale.get()];
        let helipad = (nodes.pivot_point(HELIPAD, &pristine)).map(ground);
        let dock = (nodes.pivot_point(DOCK_ACTION, &pristine)).map(ground);
        let airfield = parking.and_then(|parking| {
            let point = |name: String| nodes.pivot_point(&name, &pristine).map(ground);
            let mut places = Vec::new();
            for row in 1..=parking.rows {
                for col in 1..=parking.cols {
                    let hangar = format!("Runway{col}Park{row}Han");
                    let (parked, hangar) = if parking.hangars {
                        (hangar, None)
                    } else {
                        (format!("Runway{col}Parking{row}"), point(hangar))
                    };
                    let [at, way] = nodes.pivot_pose(&parked, &pristine)?;
                    places.push(PlaceBones {
                        at: ground(at),
                        angle: libm::atan2f(-way.z, way.x).to_degrees(),
                        hangar,
                        prep: point(format!("Runway{col}Prep{row}"))?,
                        runway: col - 1,
                    });
                }
            }
            let runways = (1..=parking.cols).map(|col| {
                Some([
                    point(format!("RunwayStart{col}"))?,
                    point(format!("RunwayEnd{col}"))?,
                ])
            });
            Some(AirfieldBones {
                places,
                runways: runways.collect::<Option<_>>()?,
            })
        });
        Ok(ModelRest {
            bones: ModelBones {
                pivot: pivot(draw.turret.as_ref())?,
                pitch_pivot: pivot(draw.turret_pitch.as_ref())?,
                barrels,
            },
            track_width,
            helipad,
            airfield,
            dock,
        })
    }

    /// The poses of a state of `animation` as its bones stand at rest: its first animation's at
    /// its first frame, or its last of `PRISTINE_BONE_POS_IN_FINAL_FRAME`, as
    /// `validateCachedBones` sets them; none for a state of no animation the package holds.
    fn rested(
        &mut self,
        animation: &IniAnimation,
    ) -> Result<Option<Vec<RigidPose>>, ZeroHourError> {
        let ends = match animation.clips.first() {
            Some(clip) => self.animation_ends(&clip.name)?,
            None => None,
        };
        let last = animation.flags.holds(AnimationFlag::PristineFinal);
        Ok(ends.map(|ends| {
            let end = if last { &ends.last } else { &ends.first };
            end.clone()
        }))
    }

    /// The fire points of a garrison of `draws`, at `scale`, on the ground in its object's frame,
    /// the game's axes, as `loadGarrisonPoints` reads its points while garrisoned and undamaged:
    /// of each module of the `W3DModelDraw` family in turn, the bones `FIREPOINT01` on of the model
    /// of its state drawn while garrisoned, up to the first it lacks, as that state's bones stand
    /// at rest, `MOST_FIRE_POINTS` at most (`Drawable::getPristineBonePositions`); none of a model
    /// the package lacks.
    fn fire_points(
        &mut self,
        draws: &[DrawModule],
        scale: ModelScale,
    ) -> Result<Vec<[f32; 2]>, ZeroHourError> {
        let mut points = Vec::new();
        for draw in draws.iter().filter(|draw| draw.kind == DrawKind::Family) {
            let state = draw.best_state(|flag| flag == "garrisoned");
            let Some(model) = state.drawn.model else {
                continue;
            };
            let animated = self.rested(state.animation)?;
            let WrittenModel::Nodes { nodes, .. } = self.model(model)? else {
                continue;
            };
            let pristine = nodes.pristine(animated.as_deref());
            for number in 1.. {
                if points.len() == MOST_FIRE_POINTS {
                    return Ok(points);
                }
                let bone = format!("FIREPOINT{number:02}");
                let Some(at) = nodes.pivot_point(&bone, &pristine) else {
                    break;
                };
                points.push([at.x * scale.get(), -at.z * scale.get()]);
            }
        }
        Ok(points)
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
        // An object fires no weapon, so its models show no rounds and no flash.
        let module = ModuleModels {
            scale,
            upgrades: &[],
            rounds_shown: [false; 3],
            recoil: BarrelRecoil::of(IniRecoil::default()),
        };
        let own = self.look_models(drawn(LookConditions::default()), module)?;
        let mut skipped = own.skipped;
        let mut variants = Vec::new();
        for conditions in LookConditions::VARIANTS {
            let looked = self.look_models(drawn(conditions), module)?;
            skipped |= looked.skipped;
            variants.push(LookVariant {
                conditions: conditions.names(),
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

    /// The models of `drawn`, each with the look whose hidden nodes it hides, of `module`'s scale,
    /// the nodes each of its upgrades shows and hides, and the rounds of each weapon slot it
    /// shows, of those that draw one the package holds; whether the package lacks one it names;
    /// and its weapon slots' barrels, where their fire effects play and their flashes show.
    fn look_models<'d>(
        &mut self,
        drawn: impl IntoIterator<Item = DrawnState<'d>>,
        module: ModuleModels<'_>,
    ) -> Result<LookModels, ZeroHourError> {
        let ModuleModels {
            scale,
            upgrades,
            rounds_shown,
            recoil,
        } = module;
        let mut looked = LookModels {
            models: Vec::new(),
            skipped: false,
            fire_bones: Vec::new(),
        };
        for DrawnState { model, look } in drawn {
            let Some(model) = model else {
                continue;
            };
            match self.model(model)? {
                WrittenModel::Nodes { nodes, bounds } => {
                    // A slot's fire effect plays at its barrels of the first model that holds any.
                    let place =
                        u8::try_from(looked.models.len()).expect("a state's models fit a u8");
                    for (slot, bones) in (0_u8..).zip(&look.weapons) {
                        if looked.fire_bones.iter().any(|bone| bone.slot == slot) {
                            continue;
                        }
                        let barrels = nodes.fire_barrels(bones, recoil);
                        if !barrels.is_empty() {
                            looked.fire_bones.push(FireBone {
                                slot,
                                model: place,
                                barrels,
                            });
                        }
                    }
                    let hide = nodes.hidden(look);
                    let top = bounds
                        .top(&hide)
                        .and_then(|top| ModelTop::new(top * scale.get()));
                    let mut node_upgrades = Vec::new();
                    for SubObjectsUpgrade { tag, changes } in upgrades {
                        let NodeChanges { show, hide } = nodes.changes(changes, look);
                        if show.is_empty() && hide.is_empty() {
                            continue;
                        }
                        node_upgrades.push(NodeUpgrade {
                            condition: tag.clone(),
                            show,
                            hide,
                        });
                    }
                    let shown = (0_u8..).zip(&look.weapons).zip(rounds_shown);
                    let rounds = shown
                        .filter(|&(_, shown)| shown)
                        .map(|((slot, bones), _)| ClipRounds {
                            slot,
                            rounds: nodes.rounds(bones, look),
                        })
                        .filter(|clip| !clip.rounds.is_empty())
                        .collect();
                    looked.models.push(ClientModel {
                        rounds,
                        model: game_path::model(model)?,
                        scale,
                        hide,
                        top,
                        upgrades: node_upgrades,
                        bone: None,
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
    pub(super) fn of(object: &IniObject) -> Vec<Self> {
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
