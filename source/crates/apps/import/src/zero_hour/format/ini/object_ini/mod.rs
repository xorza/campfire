use std::collections::BTreeMap;
use std::ops::Index;

use campfire_client_files::ModelScale;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{FlagList, IniLine, IniText};
use crate::zero_hour::format::ini::object_ini::build_variations::BuildVariations;
use crate::zero_hour::format::ini::object_ini::fight_data::{FightBlock, FightData};
use crate::zero_hour::format::ini::object_ini::ini_animation::IniAnimation;
use crate::zero_hour::format::ini::object_ini::ini_build::{IniBuild, ProductionModule};
use crate::zero_hour::format::ini::object_ini::ini_contain::{ContainClass, IniContain};
use crate::zero_hour::format::ini::object_ini::ini_death::IniDeath;
use crate::zero_hour::format::ini::object_ini::ini_experience::{IniExperience, IniStart};
use crate::zero_hour::format::ini::object_ini::ini_fall::IniFall;
use crate::zero_hour::format::ini::object_ini::ini_power::{IniPowers, PowerModule};
use crate::zero_hour::format::ini::object_ini::ini_recoil::IniRecoil;
use crate::zero_hour::format::ini::object_ini::ini_supply::{IniSupply, SupplyModule};
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::{
    IniUpgradeModule, UpgradeEffect,
};
use crate::zero_hour::format::ini::object_ini::ini_voices::IniVoices;
use crate::zero_hour::format::ini::object_ini::object_kinds::{ObjectKind, ObjectKinds};
use crate::zero_hour::format::look_conditions::LookConditions;

pub(crate) mod build_variations;
pub(crate) mod fight_data;
pub(crate) mod ini_animation;
pub(crate) mod ini_build;
pub(crate) mod ini_contain;
pub(crate) mod ini_death;
pub(crate) mod ini_experience;
pub(crate) mod ini_fall;
pub(crate) mod ini_power;
pub(crate) mod ini_recoil;
pub(crate) mod ini_supply;
pub(crate) mod ini_upgrade_module;
pub(crate) mod ini_voices;
pub(crate) mod object_kinds;

/// The objects of Zero Hour's object INI files, as `ThingFactory` reads them, each with what its
/// draw modules show by default, and their build variations, resolved once every file is read, as
/// a variation may name an object a later file defines.
#[derive(Debug, Default)]
pub(crate) struct ObjectIni {
    files: ObjectFiles,
    variations: BuildVariations,
}

/// The object INI files read so far, in their order: what `finish` makes the `ObjectIni` of.
#[derive(Debug, Default)]
pub(crate) struct ObjectFiles {
    /// Each object, in the order the files define them, at its id.
    objects: Vec<IniObject>,
    /// Each object's id by its name, of its case, as `ThingFactory::findTemplate` finds it.
    by_name: BTreeMap<String, ObjectId>,
    /// The files read so far.
    files: u32,
}

/// An object INI file, by its place in the order the files are read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct IniFile(u32);

/// An object of the INI files, by its place in their order: what every stage names an object by,
/// as the files hold one object of a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ObjectId(u32);

/// An object as the game holds it once read, a reskin a copy of its base with its own fields
/// over it (`ThingTemplate::copyFrom`): its name as the INI writes it, its draw modules, and the
/// fields that say how the game draws, places and moves it: its kinds, its fence's width, its
/// asset scale, whether it is a landmark bridge, its geometry and its normal locomotors by name,
/// each the game's default when absent; and its upgrade modules, in the order it gives them.
#[derive(Debug, Clone)]
pub(crate) struct IniObject {
    pub(crate) id: ObjectId,
    pub(crate) name: String,
    /// The file that defines it, a reskin's own.
    file: IniFile,
    draws: Vec<DrawModule>,
    kinds: ObjectKinds,
    fence_width: f32,
    scale: ModelScale,
    is_bridge: bool,
    geometry: Geometry,
    /// The locomotors of its `SET_NORMAL`, in the order its line names them, and of its
    /// `SET_TAXIING`, by which a jet moves on the ground.
    locomotors: Vec<String>,
    taxi_locomotors: Vec<String>,
    build: IniBuild,
    supply: IniSupply,
    fight: FightData,
    upgrades: Vec<IniUpgradeModule>,
    deaths: Vec<IniDeath>,
    /// Its `PhysicsBehavior`'s `Mass`, 1 when it has none, as `DEFAULT_MASS`.
    mass: f32,
    /// Its `PhysicsBehavior`'s `LateralFriction`, a share a second, none when it has none.
    lateral_friction: Option<f32>,
    /// Its `LifetimeUpdate`'s `MinLifetime` and `MaxLifetime`, in milliseconds.
    lifetime_ms: Option<[f32; 2]>,
    voices: IniVoices,
    /// Its `VisionRange`, 0 when absent, and its `ShroudClearingRange`, -1 when absent, as
    /// `ThingTemplate` starts them.
    vision_range: f32,
    shroud_clearing_range: f32,
    /// The upgrade each of its `GrantUpgradeCreate`s grants it as it is made.
    granted: Vec<String>,
    /// Its `RadarPriority`.
    radar_priority: RadarPriority,
    /// Its contain module, its last when it gives several, as `Object` keeps the last, and the
    /// room it takes of a transport, its `TransportSlotCount`.
    contain: Option<IniContain>,
    transport_slots: i32,
    /// The mapped image a button that stands for it shows, its `ButtonImage`, none of none.
    button_image: Option<String>,
    experience: IniExperience,
    powers: IniPowers,
}

/// An object's `RadarPriority` (`RadarPriorityNames`): none set, `INVALID`; one the radar draws
/// for no one; one it draws for its controller alone; or one it draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RadarPriority {
    #[default]
    Unset,
    NotOnRadar,
    LocalUnitOnly,
    Shown,
}

/// An object's geometry as the game reads it (`GeometryInfo`): its shape, its radii and its
/// height, and whether it is small, which the pathfinder routes over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geometry {
    pub(crate) shape: GeometryShape,
    /// Its radius, or a box's half extent along the object's `x`.
    pub(crate) major: f32,
    /// A box's half extent along the object's `y`.
    pub(crate) minor: f32,
    pub(crate) height: f32,
    pub(crate) small: bool,
}

/// The shapes of `GeometryNames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeometryShape {
    Sphere,
    Cylinder,
    Box,
}

/// Who draws an object a map places, as a multiplayer game spawns it (09-zero-hour.md, The
/// multiplayer rule for client objects): a unit of the sim placed before every other, as the game
/// spawns bridge-like objects in a pass of their own; the client's tree buffer; its prop buffer; or
/// the sim, as a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacedAs {
    Bridge,
    Tree,
    Prop,
    Unit,
}

/// A draw module: how it names its model, the model of its default state, if it draws one, that
/// state's look and animation, and the bones its turret turns and pitches, its `Turret` and its
/// `TurretPitch`; its barrels' recoil; the sets of flags its default state is drawn for, none and
/// its aliases'; its other condition states, in order; the map conditions it ignores
/// (`IgnoreConditionStates`); and its transition states.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DrawModule {
    pub(crate) kind: DrawKind,
    /// Its default state's model, in lowercase.
    pub(crate) model: Option<String>,
    pub(crate) look: StateLook,
    animation: IniAnimation,
    pub(crate) turret: Option<String>,
    pub(crate) turret_pitch: Option<String>,
    /// The weapon slots whose clips its models show the rounds of,
    /// `ProjectileBoneFeedbackEnabledSlots`.
    pub(crate) rounds_shown: [bool; 3],
    pub(crate) recoil: IniRecoil,
    /// The particle systems its treads throw as its object moves, as a `W3DTankDraw`, the
    /// `W3DOverlordTankDraw` that extends it and a `W3DTankTruckDraw` do, `TreadDebrisLeft` and
    /// `TreadDebrisRight`, `TrackDebrisDirtLeft` and `TrackDebrisDirtRight` when they name none;
    /// and the texture of the track marks it lays, its `TrackMarks`.
    pub(crate) tread_dust: Option<[String; 2]>,
    pub(crate) track_marks: Option<String>,
    sets: Vec<ConditionSet>,
    states: Vec<DrawState>,
    ignored: ConditionSet,
    transitions: Vec<DrawTransition>,
}

/// A condition state of a `W3DModelDraw` past its default: the sets of flags it is drawn for, its
/// own and its aliases', and its model, look and animation, which start as the default state's
/// when the module has a `DefaultConditionState`.
#[derive(Debug, Clone, PartialEq)]
struct DrawState {
    sets: Vec<ConditionSet>,
    model: Option<String>,
    look: StateLook,
    animation: IniAnimation,
}

/// A `TransitionState` of a `W3DModelDraw`: the keys of the states it is drawn between, from and
/// to, in lowercase, and its model, look and animation, which start as the default state's, of no
/// key, when the module has a `DefaultConditionState`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DrawTransition {
    pub(crate) from: String,
    pub(crate) to: String,
    model: Option<String>,
    look: StateLook,
    pub(crate) animation: IniAnimation,
}

/// A set of a state's condition flags, each in lowercase, in order and once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConditionSet {
    flags: Vec<String>,
}

/// A state of a draw module, its default or a later one: the sets of flags it is drawn for, its
/// model and its look, and its animation.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleState<'a> {
    pub(crate) sets: &'a [ConditionSet],
    pub(crate) drawn: DrawnState<'a>,
    pub(crate) animation: &'a IniAnimation,
}

/// The model and the look a draw module draws on a map of some conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DrawnState<'a> {
    pub(crate) model: Option<&'a str>,
    pub(crate) look: &'a StateLook,
}

/// What a default state hides and shows, and the bones of its weapons, by slot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StateLook {
    pub(crate) hide_show: Vec<HideShow>,
    pub(crate) weapons: [WeaponBones; 3],
}

/// A sub-object a state hides or shows, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HideShow {
    pub(crate) name: String,
    pub(crate) hide: bool,
}

/// A weapon slot's bones, each a base name the game numbers from 01, and the sub-object its
/// `WeaponHideShowBone` names, which shows its clip's rounds as one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WeaponBones {
    pub(crate) fire_fx: Option<String>,
    pub(crate) recoil: Option<String>,
    pub(crate) muzzle_flash: Option<String>,
    pub(crate) launch: Option<String>,
    pub(crate) rounds: Option<String>,
}

/// The draw modules of the `W3DModelDraw` family, whose default state names the model.
const MODEL_DRAWS: [&str; 11] = [
    "w3dmodeldraw",
    "w3ddependencymodeldraw",
    "w3doverlordaircraftdraw",
    "w3doverlordtankdraw",
    "w3doverlordtruckdraw",
    "w3dpolicecardraw",
    "w3dsciencemodeldraw",
    "w3dsupplydraw",
    "w3dtankdraw",
    "w3dtanktruckdraw",
    "w3dtruckdraw",
];

/// The locomotor set an object moves by, and the one a jet taxis by, of `TheLocomotorSetNames`.
const SET_NORMAL: &str = "set_normal";
const SET_TAXIING: &str = "set_taxiing";

/// The keywords that open a block wherever they start a line.
const OPEN_WITH_VALUE: [&str; 8] = [
    "draw",
    "behavior",
    "body",
    "clientupdate",
    "conditionstate",
    "transitionstate",
    "addmodule",
    "replacemodule",
];

/// The keywords that open a block only alone on their line: with a value, each is a field, as
/// a draw state's `Turret = Turret` is.
const OPEN_ALONE: [&str; 14] = [
    "defaultconditionstate",
    "armorset",
    "weaponset",
    "unitspecificsounds",
    "unitspecificfx",
    "prerequisites",
    "turret",
    "altturret",
    "attackareadecal",
    "targetingreticledecal",
    "griddecaltemplate",
    "deliverydecal",
    "inheritablemodule",
    "overrideablebylikekind",
];

/// How a draw module names its model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DrawKind {
    /// The `Model` of its default state, as the `W3DModelDraw` family does.
    Family,
    /// The `ModelName` of a `W3DTreeDraw`, which the client's tree buffer draws.
    Tree,
    /// The `ModelName` of a `W3DPropDraw`.
    Prop,
    /// It draws no model.
    Other,
}

/// The weapon slots, as a field names them.
pub(crate) const SLOTS: [&str; 3] = ["primary", "secondary", "tertiary"];

/// A block the reader is inside.
#[derive(Debug)]
enum Block {
    Object,
    /// A draw module, with the count of its states begun, and whether its first was a
    /// `DefaultConditionState`, which every later state starts as a copy of.
    Draw {
        kind: DrawKind,
        states: u32,
        copied: bool,
    },
    /// The first state of such a module, its default, and whether it named its model.
    DefaultState {
        named: bool,
    },
    /// A later state of such a module, and whether it holds a model, its own or its copy's.
    State {
        named: bool,
    },
    /// A `TransitionState` of such a module, and whether it holds a model, its own or its copy's.
    Transition {
        named: bool,
    },
    /// An object's `Prerequisites`.
    Prerequisites,
    /// A block combat reads: a body, a weapon set or an armor set.
    Fight(FightBlock),
    /// A behavior module whose fields production reads.
    Production(ProductionModule),
    /// A behavior module whose fields supply reads.
    Supply(SupplyModule),
    /// A special power's or a heal burst's module.
    Power(PowerModule),
    /// An upgrade module, the object's last.
    Upgrade,
    /// A `PhysicsBehavior`, whose mass a wreck reads.
    Physics,
    /// A `LifetimeUpdate`, whose lifetime a wreck reads.
    Lifetime,
    /// A `GrantUpgradeCreate`, whose upgrade the object is made with.
    GrantUpgrade,
    /// A `VeterancyGainCreate`, whose level the object is made at.
    VeterancyGain,
    /// A slow death module, the object's last.
    Death,
    /// A contain module, the object's last.
    Contain,
    /// An object's `UnitSpecificSounds`.
    UnitSounds,
    Other,
}

/// A file as it is read: the blocks the reader is inside, the object and the draw module open.
#[derive(Debug, Default)]
struct Reading {
    stack: Vec<Block>,
    object: Option<OpenObject>,
    draw: Option<DrawModule>,
}

/// An object being read: whether it reskins another, whose fields its table cannot set, and
/// whether its draw modules are still its base's, which its first own one drops.
#[derive(Debug)]
struct OpenObject {
    object: IniObject,
    reskin: bool,
    copied_draws: bool,
}

impl ObjectFiles {
    /// Reads one INI file, adding its objects to those read before. An object of a name read
    /// before, and a reskin of a base not yet read, are refused: no known install holds either,
    /// where the game's release build parses the second definition into the first, and refuses
    /// a reskin of no base.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut reading = Reading::default();
        for line in text.lines().iter() {
            let word = line.word();
            if word == "end" {
                if let Some(done) = reading.end(line.number)? {
                    debug_assert_eq!(done.id, self.next_id(), "an object opens with the next id");
                    self.by_name.insert(done.name.clone(), done.id);
                    self.objects.push(done);
                }
            } else if reading.stack.is_empty() {
                reading.open(self.object_of(word, &line)?);
            } else {
                reading.line(word, &line)?;
            }
        }
        self.files += 1;
        if reading.stack.is_empty() {
            Ok(())
        } else {
            Err(IniError::Unclosed)
        }
    }

    /// The objects read, with their build variations resolved.
    pub(crate) fn finish(self) -> ObjectIni {
        let variations = BuildVariations::of(&self);
        ObjectIni {
            files: self,
            variations,
        }
    }

    /// Every object, in the order of their names.
    pub(crate) fn objects(&self) -> impl Iterator<Item = &IniObject> {
        self.by_name.values().map(|&id| &self[id])
    }

    /// The object of the name `name`, of its case; none for a name no object has.
    pub(crate) fn named(&self, name: &str) -> Option<&IniObject> {
        self.by_name.get(name).map(|&id| &self[id])
    }

    /// The id the next object read takes.
    fn next_id(&self) -> ObjectId {
        ObjectId(
            u32::try_from(self.objects.len()).expect("an install holds fewer than 2³² objects"),
        )
    }

    /// The object a top-level line of `word` opens: a new one, or a copy of the reskin's base.
    fn object_of(&self, word: &str, at: &IniLine<'_>) -> Result<OpenObject, IniError> {
        let line = at.number;
        let base = match word {
            "object" => None,
            "objectreskin" => Some(*at.tokens.get(2).ok_or(IniError::NoName { line })?),
            _ => return Err(IniError::NotBlock { line }),
        };
        let name = *at.tokens.get(1).ok_or(IniError::NoName { line })?;
        if self.by_name.contains_key(name) {
            return Err(IniError::Redefined { line });
        }
        let id = self.next_id();
        let object = match base {
            None => IniObject {
                id,
                name: name.to_owned(),
                file: IniFile(self.files),
                draws: Vec::new(),
                kinds: ObjectKinds::default(),
                fence_width: 0.0,
                scale: ModelScale::ONE,
                is_bridge: false,
                geometry: Geometry::DEFAULT,
                locomotors: Vec::new(),
                taxi_locomotors: Vec::new(),
                build: IniBuild::default(),
                supply: IniSupply::default(),
                fight: FightData::default(),
                upgrades: Vec::new(),
                deaths: Vec::new(),
                mass: 1.0,
                lateral_friction: None,
                lifetime_ms: None,
                voices: IniVoices::default(),
                vision_range: 0.0,
                shroud_clearing_range: -1.0,
                granted: Vec::new(),
                radar_priority: RadarPriority::Unset,
                contain: None,
                transport_slots: 0,
                button_image: None,
                experience: IniExperience::default(),
                powers: IniPowers::default(),
            },
            Some(base) => IniObject {
                id,
                name: name.to_owned(),
                file: IniFile(self.files),
                ..self.named(base).ok_or(IniError::NoBase { line })?.clone()
            },
        };
        Ok(OpenObject {
            object,
            reskin: base.is_some(),
            copied_draws: base.is_some(),
        })
    }

    /// Whether the line of `word` and `tokens` opens a block.
    fn opens(word: &str, tokens: &[&str]) -> bool {
        OPEN_WITH_VALUE.contains(&word) || (tokens.len() == 1 && OPEN_ALONE.contains(&word))
    }

    /// A model field's value, in lowercase, as the game finds a model's file whatever its case:
    /// none for `NONE`.
    fn model(tokens: &[&str]) -> Option<String> {
        tokens
            .get(1)
            .filter(|model| !model.eq_ignore_ascii_case("none"))
            .map(|model| model.to_ascii_lowercase())
    }
}

impl Index<ObjectId> for ObjectFiles {
    type Output = IniObject;

    fn index(&self, id: ObjectId) -> &IniObject {
        &self.objects[usize::try_from(id.0).expect("a u32 fits usize")]
    }
}

impl ObjectIni {
    /// Every object, in the order of their names.
    pub(crate) fn objects(&self) -> impl Iterator<Item = &IniObject> {
        self.files.objects()
    }

    /// The object a map's template names, of its case, as `findTemplate` finds it; none for a
    /// template no INI object names, which the game spawns nothing of.
    pub(crate) fn object(&self, template: &[u8]) -> Option<&IniObject> {
        let name: String = template.iter().map(|&byte| char::from(byte)).collect();
        self.named(&name)
    }

    /// The object of the name `name`, of its case; none for a name no object has.
    pub(crate) fn named(&self, name: &str) -> Option<&IniObject> {
        self.files.named(name)
    }

    /// The objects' build variations, as the import converts them.
    pub(crate) const fn variations(&self) -> &BuildVariations {
        &self.variations
    }

    /// The build variations of `parent`, in its line's order; none for an object of none.
    pub(crate) fn variations_of(
        &self,
        parent: &IniObject,
    ) -> Option<impl ExactSizeIterator<Item = &IniObject>> {
        let of_parent = self.variations.of_parent(parent)?;
        Some(of_parent.iter().map(|&variation| &self[variation]))
    }

    /// The object whose fields `object`'s unit type takes but its body and look: a parent's
    /// first build variation, else the object itself.
    pub(crate) fn fielded<'a>(&'a self, object: &'a IniObject) -> &'a IniObject {
        match self.variations.of_parent(object) {
            Some(of_parent) => &self[of_parent[0]],
            None => object,
        }
    }
}

impl Index<ObjectId> for ObjectIni {
    type Output = IniObject;

    fn index(&self, id: ObjectId) -> &IniObject {
        &self.files[id]
    }
}

impl Geometry {
    /// A thing's geometry until its fields say otherwise: a sphere of radius 1, not small.
    const DEFAULT: Self = Self {
        shape: GeometryShape::Sphere,
        major: 1.0,
        minor: 1.0,
        height: 1.0,
        small: false,
    };

    /// Its height, as `GeometryInfo::set` takes it: a sphere's its radius, whatever its
    /// `GeometryHeight`.
    pub(crate) const fn height(&self) -> f32 {
        match self.shape {
            GeometryShape::Sphere => self.major,
            GeometryShape::Cylinder | GeometryShape::Box => self.height,
        }
    }

    /// The radius of the circle round it on the ground, as `calcBoundingStuff` computes it: a
    /// box's `√(major² + minor²)`, each step in the game's `f32`, else its major radius.
    #[expect(
        clippy::imprecise_flops,
        reason = "the game's own formula, rounded at each step as the game rounds it"
    )]
    pub(crate) fn bounding_circle(&self) -> f32 {
        match self.shape {
            GeometryShape::Sphere | GeometryShape::Cylinder => self.major,
            GeometryShape::Box => (self.major * self.major + self.minor * self.minor).sqrt(),
        }
    }

    /// The radius of the sphere round it, as `calcBoundingStuff` computes it: a box's
    /// `√(major² + minor² + (height / 2)²)`, a cylinder's larger of half its height and its
    /// radius, a sphere's radius; each step in the game's `f32`.
    pub(crate) fn bounding_sphere(&self) -> f32 {
        let half = self.height * 0.5;
        match self.shape {
            GeometryShape::Sphere => self.major,
            GeometryShape::Cylinder => half.max(self.major),
            GeometryShape::Box => {
                (self.major * self.major + self.minor * self.minor + half * half).sqrt()
            }
        }
    }

    /// Takes a geometry field of `word`; whether `word` is one.
    fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<bool, IniError> {
        match word {
            "geometry" => {
                let value = at.value()?.to_ascii_lowercase();
                self.shape = match value.as_str() {
                    "sphere" => GeometryShape::Sphere,
                    "cylinder" => GeometryShape::Cylinder,
                    "box" => GeometryShape::Box,
                    _ => return Err(IniError::NotListed { line: at.number }),
                };
            }
            "geometrymajorradius" => self.major = at.real()?,
            "geometryminorradius" => self.minor = at.real()?,
            "geometryheight" => self.height = at.real()?,
            "geometryissmall" => self.small = at.yes()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
}

impl DrawModule {
    /// A module of `kind`, of no state yet.
    fn new(kind: DrawKind) -> Self {
        Self {
            kind,
            model: None,
            look: StateLook::default(),
            turret: None,
            turret_pitch: None,
            rounds_shown: [false; 3],
            recoil: IniRecoil::default(),
            tread_dust: None,
            track_marks: None,
            animation: IniAnimation::default(),
            sets: vec![ConditionSet::default()],
            states: Vec::new(),
            ignored: ConditionSet::default(),
            transitions: Vec::new(),
        }
    }

    /// The model and the look it draws on a map of `conditions`, those it ignores dropped: of
    /// its state whose set of flags best matches them, as `SparseMatchFinder::findBestInfoSlow`
    /// finds it: the set of the most flags in common, of a tie the one of the fewest the map
    /// lacks, of a tie the first, the states in order, the default first.
    pub(crate) fn drawn(&self, conditions: LookConditions) -> DrawnState<'_> {
        let held = |flag: &str| match flag {
            "night" => conditions.night,
            "snow" => conditions.snow,
            _ => false,
        };
        self.best_state(held).drawn
    }

    /// Its state drawn while the flags `held` holds are set, as `drawn` finds it.
    pub(crate) fn best_state(&self, held: impl Fn(&str) -> bool) -> ModuleState<'_> {
        let held = |flag: &String| held(flag) && !self.ignored.flags.contains(flag);
        let mut best = None;
        let (mut most, mut fewest) = (0, usize::MAX);
        for state in self.states() {
            for set in state.sets {
                let common = set.flags.iter().filter(|flag| held(flag)).count();
                let lacked = set.flags.len() - common;
                if common > most || (common >= most && lacked < fewest) {
                    (best, most, fewest) = (Some(state), common, lacked);
                }
            }
        }
        best.expect("the default state's set is drawn for its first")
    }

    /// Its states, its default first, then the others in order.
    pub(crate) fn states(&self) -> impl Iterator<Item = ModuleState<'_>> {
        let default = ModuleState {
            sets: &self.sets,
            drawn: DrawnState {
                model: self.model.as_deref(),
                look: &self.look,
            },
            animation: &self.animation,
        };
        let states = self.states.iter().map(|state| ModuleState {
            sets: &state.sets,
            drawn: DrawnState {
                model: state.model.as_deref(),
                look: &state.look,
            },
            animation: &state.animation,
        });
        std::iter::once(default).chain(states)
    }

    /// The flags it ignores as it matches its states, `IgnoreConditionStates`.
    pub(crate) fn ignored(&self) -> &[String] {
        &self.ignored.flags
    }

    pub(crate) fn transitions(&self) -> &[DrawTransition] {
        &self.transitions
    }

    /// A field of its default state that the look reads.
    fn default_field(&mut self, word: &str, tokens: &[&str], line: usize) -> Result<(), IniError> {
        let bone = match word {
            "turret" => Some(&mut self.turret),
            "turretpitch" => Some(&mut self.turret_pitch),
            _ => None,
        };
        if let (Some(bone), Some(named)) = (bone, tokens.get(1)) {
            *bone = Some((*named).to_owned());
        }
        state_field(&mut self.model, &mut self.look, word, tokens, line)
    }

    /// The state the reader is in past the default, its last.
    fn last_state(&mut self) -> &mut DrawState {
        self.states
            .last_mut()
            .expect("a state's block is in its module")
    }
}

impl DrawTransition {
    /// The model and the look it draws.
    pub(crate) fn drawn(&self) -> DrawnState<'_> {
        DrawnState {
            model: self.model.as_deref(),
            look: &self.look,
        }
    }
}

impl ConditionSet {
    /// The set of flags of a state's line `at`, from its second token, as `ModelConditionFlags`
    /// reads them: `NONE` none, a name of either case once however often it is named.
    fn of(at: &IniLine<'_>) -> Result<Self, IniError> {
        let names: Vec<&str> = match at.flags()? {
            FlagList::Clear => Vec::new(),
            FlagList::Only(names) => names,
            FlagList::Change(changes) => changes
                .into_iter()
                .filter(|change| change.set)
                .map(|change| change.name)
                .collect(),
        };
        let mut flags: Vec<String> = names.iter().map(|name| name.to_ascii_lowercase()).collect();
        flags.sort_unstable();
        flags.dedup();
        Ok(Self { flags })
    }

    pub(crate) fn flags(&self) -> &[String] {
        &self.flags
    }
}

/// A field of a condition state that the look reads, into its `model` and `look`.
fn state_field(
    model: &mut Option<String>,
    look: &mut StateLook,
    word: &str,
    tokens: &[&str],
    line: usize,
) -> Result<(), IniError> {
    match word {
        "model" => *model = ObjectFiles::model(tokens),
        "hidesubobject" | "showsubobject" => {
            look.hide_show(&tokens[1..], word == "hidesubobject");
        }
        "weaponfirefxbone" | "weaponrecoilbone" | "weaponmuzzleflash" | "weaponlaunchbone"
        | "weaponhideshowbone" => {
            let (Some(slot), Some(bone)) = (tokens.get(1), tokens.get(2)) else {
                return Err(IniError::WeaponSlot { line });
            };
            let slot = SLOTS
                .iter()
                .position(|known| slot.eq_ignore_ascii_case(known))
                .ok_or(IniError::WeaponSlot { line })?;
            let bones = &mut look.weapons[slot];
            let field = match word {
                "weaponfirefxbone" => &mut bones.fire_fx,
                "weaponrecoilbone" => &mut bones.recoil,
                "weaponmuzzleflash" => &mut bones.muzzle_flash,
                "weaponhideshowbone" => &mut bones.rounds,
                _ => &mut bones.launch,
            };
            *field = Some((*bone).to_owned()).filter(|bone| !bone.eq_ignore_ascii_case("none"));
        }
        _ => {}
    }
    Ok(())
}

impl StateLook {
    /// Hides or shows `names`, as `parseShowHideSubObject` does: `NONE` first empties the list,
    /// and a name the list holds keeps its place and takes the new change.
    fn hide_show(&mut self, names: &[&str], hide: bool) {
        let list = &mut self.hide_show;
        if names
            .first()
            .is_some_and(|name| name.eq_ignore_ascii_case("none"))
        {
            list.clear();
            return;
        }
        for name in names {
            match list
                .iter_mut()
                .find(|known| known.name.eq_ignore_ascii_case(name))
            {
                Some(known) => known.hide = hide,
                None => list.push(HideShow {
                    name: (*name).to_owned(),
                    hide,
                }),
            }
        }
    }
}

impl IniObject {
    pub(crate) fn draws(&self) -> &[DrawModule] {
        &self.draws
    }

    pub(crate) const fn file(&self) -> IniFile {
        self.file
    }

    /// Whether `other` differs from it in its body and its look alone: its geometry, its draw
    /// modules and its scale (09-zero-hour.md, A variation varies the body and the look alone).
    pub(crate) fn varies_alone(&self, other: &Self) -> bool {
        self.kinds == other.kinds
            && self.fence_width == other.fence_width
            && self.is_bridge == other.is_bridge
            && self.locomotors == other.locomotors
            && self.taxi_locomotors == other.taxi_locomotors
            && self.build == other.build
            && self.supply == other.supply
            && self.fight == other.fight
    }

    /// Who draws an object of it a map places: a bridge-like object, a landmark bridge or a wall
    /// a unit walks on, the sim before every other; an optimized tree the tree buffer; a prop, or
    /// fluff, an object a build clears whose fence is 0 wide, the prop buffer, as a multiplayer
    /// game forces fluff onto it; any other the sim.
    pub(crate) const fn placed_as(&self) -> PlacedAs {
        let kinds = self.kinds;
        if self.is_bridge || kinds.holds(ObjectKind::WalkOnTopOfWall) {
            PlacedAs::Bridge
        } else if kinds.holds(ObjectKind::OptimizedTree) {
            PlacedAs::Tree
        } else if kinds.holds(ObjectKind::Prop)
            || (kinds.holds(ObjectKind::ClearedByBuild) && self.fence_width == 0.0)
        {
            PlacedAs::Prop
        } else {
            PlacedAs::Unit
        }
    }

    /// The geometry of its `Geometry` fields.
    pub(crate) const fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// The names of the locomotors it moves by, its `SET_NORMAL`'s.
    pub(crate) fn locomotors(&self) -> &[String] {
        &self.locomotors
    }

    /// The names of the locomotors it taxis by, its `SET_TAXIING`'s.
    pub(crate) fn taxi_locomotors(&self) -> &[String] {
        &self.taxi_locomotors
    }

    /// What it gives production: its cost, time, command set, prerequisites and modules.
    pub(crate) const fn build(&self) -> &IniBuild {
        &self.build
    }

    /// What it gives supply: its dock, its center, its gatherer and its spawns.
    pub(crate) const fn supply(&self) -> &IniSupply {
        &self.supply
    }

    /// What it gives combat: its body, its weapon sets, its armor sets, its shell and its turret.
    pub(crate) const fn fight(&self) -> &FightData {
        &self.fight
    }

    /// The voices it answers with.
    pub(crate) const fn voices(&self) -> &IniVoices {
        &self.voices
    }

    /// The slow death a normal death of it most likely takes, as `SlowDeathBehavior::onDie` draws
    /// one by weight among those that apply: of the most weight, the first of a tie; none when
    /// none applies, as it is then destroyed at once.
    pub(crate) fn slow_death(&self) -> Option<&IniDeath> {
        let applies = self
            .deaths
            .iter()
            .filter(|death| death.effect.is_none() && death.fall.is_none() && death.applies());
        applies.rev().max_by_key(|death| death.weight)
    }

    /// The fall a normal death of it takes, its first aircraft's slow death that applies; none
    /// when none does.
    pub(crate) fn fall(&self) -> Option<&IniFall> {
        self.fall_death()?.fall.as_ref()
    }

    /// The aircraft's slow death a normal death of it takes, its first that applies; none when
    /// none does.
    pub(crate) fn fall_death(&self) -> Option<&IniDeath> {
        let mut applies = self.deaths.iter().filter(|death| death.applies());
        applies.find(|death| death.fall.is_some())
    }

    /// The effect list a normal death of it plays: the `DeathFX` of its first `FXListDie` that
    /// applies and names one (09-zero-hour.md, Particles and effects); none when none does.
    pub(crate) fn death_effect(&self) -> Option<&str> {
        let effects = self.deaths.iter().filter(|death| death.applies());
        effects
            .filter_map(|death| death.effect.as_deref())
            .find(|effect| !effect.is_empty() && !effect.eq_ignore_ascii_case("none"))
    }

    /// Its upgrade modules, in the order it gives them.
    /// Whether it gives its player a radar as it is made: a `RadarUpgrade` of its own whose every
    /// trigger a `GrantUpgradeCreate` of its own grants, as the USA's command center and the GLA's
    /// radar van grant theirs (02-engine-core.md, A minimap by a radar tag).
    pub(crate) fn makes_radar(&self) -> bool {
        self.upgrades.iter().any(|module| {
            module.effect == UpgradeEffect::Radar
                && !module.triggers.is_empty()
                && module
                    .triggers
                    .iter()
                    .all(|trigger| self.granted.contains(trigger))
        })
    }

    /// Its priority on the radar: its `RadarPriority`, or of none set, a structure's for one a
    /// garrison holds or that is captured, as `Object::getRadarPriority` decides, else none the
    /// radar draws.
    pub(crate) const fn radar_priority(&self) -> RadarPriority {
        match self.radar_priority {
            RadarPriority::Unset
                if self.garrisons() || self.kinds.holds(ObjectKind::Capturable) =>
            {
                RadarPriority::Shown
            }
            priority => priority,
        }
    }

    pub(crate) fn upgrades(&self) -> &[IniUpgradeModule] {
        &self.upgrades
    }

    /// Its contain module, if it has one.
    pub(crate) const fn contain(&self) -> Option<&IniContain> {
        self.contain.as_ref()
    }

    /// Whether a `GarrisonContain` holds its garrison.
    pub(crate) const fn garrisons(&self) -> bool {
        matches!(
            &self.contain,
            Some(IniContain {
                class: ContainClass::Garrison,
                ..
            })
        )
    }

    /// The room it takes of a transport, its `TransportSlotCount`: 0 for one no transport takes.
    pub(crate) const fn transport_slots(&self) -> i32 {
        self.transport_slots
    }

    /// The mapped image a button that stands for it shows, its `ButtonImage`, as a transport's
    /// exit shows its rider (`ControlBar::populateButtonProc`).
    pub(crate) fn button_image(&self) -> Option<&str> {
        self.button_image.as_deref()
    }

    /// Its kinds of `KindOf` that the import reads.
    pub(crate) const fn kinds(&self) -> ObjectKinds {
        self.kinds
    }

    /// Whether it counts for a multiplayer victory: a structure of the kind
    /// `MP_COUNT_FOR_VICTORY`, as `Team::hasAnyBuildings` counts it.
    pub(crate) const fn counts_for_victory(&self) -> bool {
        self.kinds.holds(ObjectKind::Structure) && self.kinds.holds(ObjectKind::MpCountForVictory)
    }

    /// Whether it is a structure, of `KindOf`.
    pub(crate) const fn is_structure(&self) -> bool {
        self.kinds.holds(ObjectKind::Structure)
    }

    /// Whether it is a dozer, of `KindOf`.
    pub(crate) const fn is_dozer(&self) -> bool {
        self.kinds.holds(ObjectKind::Dozer)
    }

    /// Whether it is a vehicle, of `KindOf`.
    pub(crate) const fn is_vehicle(&self) -> bool {
        self.kinds.holds(ObjectKind::Vehicle)
    }

    /// Whether it may answer an ally's call to retaliate: neither `IMMOBILE`, a `DRONE` nor
    /// `CANNOT_RETALIATE`, as `AIUpdateInterface::shouldRetaliate` refuses them.
    pub(crate) const fn may_retaliate(&self) -> bool {
        !(self.kinds.holds(ObjectKind::Immobile)
            || self.kinds.holds(ObjectKind::Drone)
            || self.kinds.holds(ObjectKind::CannotRetaliate))
    }

    /// Whether it is a hulk, of `KindOf`: a wreck of a death, which no rule reads.
    pub(crate) const fn is_hulk(&self) -> bool {
        self.kinds.holds(ObjectKind::Hulk)
    }

    /// Its mass, its `PhysicsBehavior`'s, which a force pushes it by.
    pub(crate) const fn mass(&self) -> f32 {
        self.mass
    }

    /// The share of its lateral velocity it loses a frame, its `PhysicsBehavior`'s
    /// `LateralFriction` a second times the `f32` of a frame's seconds, as `parseFrictionPerSec`
    /// reads it, or `DEFAULT_LATERAL_FRICTION`, 0.15, with none.
    pub(crate) fn lateral_friction(&self) -> f32 {
        self.lateral_friction
            .map_or(0.15, |per_second| per_second * (1.0 / 30.0))
    }

    /// Its `VisionRange`, 0 when absent.
    pub(crate) const fn vision_range(&self) -> f32 {
        self.vision_range
    }

    /// How far it clears the shroud for its player: its `ShroudClearingRange`, or its
    /// `VisionRange` when it gives none, as `Object`'s constructor takes it.
    pub(crate) fn sight_range(&self) -> f32 {
        if self.shroud_clearing_range == -1.0 {
            self.vision_range
        } else {
            self.shroud_clearing_range
        }
    }

    /// Its special powers and its heal burst.
    pub(crate) const fn powers(&self) -> &IniPowers {
        &self.powers
    }

    /// Its veterancy: what its death gives, what each level needs, whether it gains any, and the
    /// levels it is made at.
    pub(crate) const fn experience(&self) -> &IniExperience {
        &self.experience
    }

    /// The least and the most of its lifetime in milliseconds, its `LifetimeUpdate`'s; none for
    /// an object of none.
    pub(crate) const fn lifetime_ms(&self) -> Option<[f32; 2]> {
        self.lifetime_ms
    }

    /// Whether it is a wall, which a projectile of `WALLS` detonates on: a fence, of a
    /// `FenceWidth` above 0 (`shouldProjectileCollideWith`).
    pub(crate) const fn is_wall(&self) -> bool {
        self.fence_width > 0.0
    }

    /// The asset scale every drawable of it takes: its `Scale`.
    pub(crate) const fn scale(&self) -> ModelScale {
        self.scale
    }
}

impl Reading {
    fn open(&mut self, object: OpenObject) {
        self.object = Some(object);
        self.stack.push(Block::Object);
    }

    /// Closes the innermost block; the object it closes, if it is one.
    fn end(&mut self, line: usize) -> Result<Option<IniObject>, IniError> {
        match self.stack.pop().ok_or(IniError::StrayEnd { line })? {
            Block::Object => Ok(Some(
                self.object
                    .take()
                    .expect("an object block holds an object")
                    .object,
            )),
            Block::Draw { .. } => {
                let done = self.draw.take().expect("a draw block holds a module");
                let open = self.object.as_mut().expect("a draw is in an object");
                if open.copied_draws {
                    open.object.draws.clear();
                    open.copied_draws = false;
                }
                open.object.draws.push(done);
                Ok(None)
            }
            // `parseConditionState` requires each state to name its model, `NONE` or one.
            Block::DefaultState { named: false }
            | Block::State { named: false }
            | Block::Transition { named: false } => Err(IniError::NoModel { line }),
            Block::DefaultState { named: true } => {
                let module = self.draw.as_ref().expect("a state is in a draw");
                module.animation.check_state(line)?;
                Ok(None)
            }
            Block::State { named: true } => {
                let module = self.draw.as_ref().expect("a state is in a draw");
                let state = module
                    .states
                    .last()
                    .expect("a state's block is in its module");
                state.animation.check_state(line)?;
                Ok(None)
            }
            Block::Transition { named: true } => {
                let module = self.draw.as_ref().expect("a transition is in a draw");
                let transition = module
                    .transitions
                    .last()
                    .expect("a transition's block is its own");
                transition.animation.check_transition(line)?;
                Ok(None)
            }
            Block::Prerequisites
            | Block::Fight(_)
            | Block::Production(_)
            | Block::Supply(_)
            | Block::Upgrade
            | Block::Physics
            | Block::Lifetime
            | Block::GrantUpgrade
            | Block::VeterancyGain
            | Block::Power(_)
            | Block::Death
            | Block::Contain
            | Block::UnitSounds
            | Block::Other => Ok(None),
        }
    }

    /// A line other than `End`, of its first word in lowercase, `word`.
    fn line(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let (tokens, line) = (at.tokens, at.number);
        let top = self
            .stack
            .last_mut()
            .expect("a line below the top level is in a block");
        match top {
            Block::Draw {
                kind,
                states,
                copied,
            } => {
                let module = self.draw.as_mut().expect("a draw block holds a module");
                match word {
                    "defaultconditionstate" | "conditionstate" if *kind == DrawKind::Family => {
                        *states += 1;
                        let default = *states == 1;
                        if word == "defaultconditionstate" && !default {
                            return Err(IniError::LateDefaultState { line });
                        }
                        // Without `DefaultConditionState`, the game requires the first state to
                        // set no condition, as `NONE` or no flag says, which is then the default.
                        let no_condition = tokens[1..]
                            .iter()
                            .all(|flag| flag.eq_ignore_ascii_case("none"));
                        if default && word == "conditionstate" && !no_condition {
                            return Err(IniError::NoDefaultState { line });
                        }
                        if default {
                            *copied = word == "defaultconditionstate";
                            self.stack.push(Block::DefaultState { named: false });
                        } else {
                            // A later state starts as the default's copy, of its own flags.
                            let mut state = DrawState {
                                sets: vec![ConditionSet::of(at)?],
                                model: None,
                                look: StateLook::default(),
                                animation: IniAnimation::default(),
                            };
                            if *copied {
                                state.model.clone_from(&module.model);
                                state.look.clone_from(&module.look);
                                state.animation = module.animation.copied();
                            }
                            let named = state.model.is_some();
                            module.states.push(state);
                            self.stack.push(Block::State { named });
                        }
                    }
                    "aliasconditionstate" if *kind == DrawKind::Family => {
                        // An alias draws the state before it for one more set of flags.
                        let set = ConditionSet::of(at)?;
                        match (module.states.last_mut(), *states) {
                            (Some(state), _) => state.sets.push(set),
                            (None, 1..) => module.sets.push(set),
                            (None, 0) => return Err(IniError::FirstAlias { line }),
                        }
                    }
                    "transitionstate" if *kind == DrawKind::Family => {
                        // A transition starts as the default's copy, of no key of its own.
                        let (Some(from), Some(to)) = (tokens.get(1), tokens.get(2)) else {
                            return Err(IniError::Transition { line });
                        };
                        let [from, to] = [from, to].map(|key| key.to_ascii_lowercase());
                        if from == to {
                            return Err(IniError::Transition { line });
                        }
                        let mut transition = DrawTransition {
                            from,
                            to,
                            model: None,
                            look: StateLook::default(),
                            animation: IniAnimation::default(),
                        };
                        if *copied {
                            transition.model.clone_from(&module.model);
                            transition.look.clone_from(&module.look);
                            transition.animation = module.animation.copied();
                            transition.animation.key = None;
                            transition.animation.waits_for = None;
                        }
                        let named = transition.model.is_some();
                        // A pair's later transition replaces its earlier, as `m_transitionMap`
                        // takes it.
                        module.transitions.retain(|known| {
                            [&known.from, &known.to] != [&transition.from, &transition.to]
                        });
                        module.transitions.push(transition);
                        self.stack.push(Block::Transition { named });
                    }
                    _ if *kind == DrawKind::Family && module.recoil.field(word, at)? => {}
                    "ignoreconditionstates" if *kind == DrawKind::Family => {
                        module.ignored = ConditionSet::of(at)?;
                    }
                    "projectilebonefeedbackenabledslots" if *kind == DrawKind::Family => {
                        let mut shown = [false; 3];
                        for slot in &tokens[1..] {
                            let at_slot = SLOTS
                                .iter()
                                .position(|known| slot.eq_ignore_ascii_case(known));
                            shown[at_slot.ok_or(IniError::WeaponSlot { line })?] = true;
                        }
                        module.rounds_shown = shown;
                    }
                    "trackmarks" if *kind == DrawKind::Family => {
                        let texture = at.value()?;
                        module.track_marks =
                            (!texture.eq_ignore_ascii_case("none")).then(|| texture.to_owned());
                    }
                    "treaddebrisleft" | "treaddebrisright" if module.tread_dust.is_some() => {
                        let side = usize::from(word == "treaddebrisright");
                        let systems = module.tread_dust.as_mut().expect("a tank draw's");
                        at.value()?.clone_into(&mut systems[side]);
                    }
                    "modelname" if matches!(*kind, DrawKind::Tree | DrawKind::Prop) => {
                        module.model = ObjectFiles::model(tokens);
                    }
                    _ if ObjectFiles::opens(word, tokens) => self.stack.push(Block::Other),
                    _ => {}
                }
            }
            Block::State { named } => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                let state = module.last_state();
                if !state.animation.field(word, at)? {
                    state_field(&mut state.model, &mut state.look, word, tokens, line)?;
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::DefaultState { named } => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                if !module.animation.field(word, at)? {
                    module.default_field(word, tokens, line)?;
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Transition { named } => {
                let module = self.draw.as_mut().expect("a transition is in a draw");
                *named |= word == "model";
                let transition = module
                    .transitions
                    .last_mut()
                    .expect("a transition's block is its own");
                if !transition.animation.field(word, at)? {
                    let (model, look) = (&mut transition.model, &mut transition.look);
                    state_field(model, look, word, tokens, line)?;
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Object if word == "draw" => {
                let module = tokens
                    .get(1)
                    .map_or_default(|kind| kind.to_ascii_lowercase());
                let kind = match module.as_str() {
                    family if MODEL_DRAWS.contains(&family) => DrawKind::Family,
                    "w3dtreedraw" => DrawKind::Tree,
                    "w3dpropdraw" => DrawKind::Prop,
                    _ => DrawKind::Other,
                };
                let mut draw = DrawModule::new(kind);
                let tank = matches!(
                    module.as_str(),
                    "w3dtankdraw" | "w3doverlordtankdraw" | "w3dtanktruckdraw"
                );
                draw.tread_dust = tank
                    .then(|| ["TrackDebrisDirtLeft", "TrackDebrisDirtRight"].map(str::to_owned));
                self.draw = Some(draw);
                self.stack.push(Block::Draw {
                    kind,
                    states: 0,
                    copied: false,
                });
            }
            Block::Prerequisites => {
                let open = self.object.as_mut().expect("prerequisites are an object's");
                open.object.build.prerequisite(word, at)?;
            }
            Block::Fight(block) => {
                let block = *block;
                let open = self.object.as_mut().expect("a combat block is an object's");
                match open.object.fight.field(block, word, at)? {
                    Some(inner) => self.stack.push(Block::Fight(inner)),
                    None if ObjectFiles::opens(word, tokens) => self.stack.push(Block::Other),
                    None => {}
                }
            }
            Block::Production(module) => {
                let module = *module;
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.build.module_field(module, word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Upgrade => {
                let open = self.object.as_mut().expect("a module is an object's");
                let module = open.object.upgrades.last_mut();
                module
                    .expect("an upgrade block opened its module")
                    .field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Power(module) => {
                let module = *module;
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.powers.module_field(module, word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::VeterancyGain => {
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.experience.start_field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::GrantUpgrade => {
                let open = self.object.as_mut().expect("a module is an object's");
                if word == "upgradetogrant" {
                    open.object.granted.push(at.value()?.to_owned());
                }
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Physics | Block::Lifetime => {
                let lifetime = matches!(self.stack.last(), Some(Block::Lifetime));
                let open = self.object.as_mut().expect("a module is an object's");
                let object = &mut open.object;
                match word {
                    "mass" if !lifetime => object.mass = at.real()?,
                    "lateralfriction" if !lifetime => object.lateral_friction = Some(at.real()?),
                    "minlifetime" | "maxlifetime" if lifetime => {
                        let ms = object.lifetime_ms.get_or_insert([0.0; 2]);
                        ms[usize::from(word == "maxlifetime")] = at.real()?;
                    }
                    _ if ObjectFiles::opens(word, tokens) => self.stack.push(Block::Other),
                    _ => {}
                }
            }
            Block::Death => {
                let open = self.object.as_mut().expect("a module is an object's");
                let module = open.object.deaths.last_mut();
                module
                    .expect("a death block opened its module")
                    .field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Supply(module) => {
                let module = *module;
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.supply.module_field(module, word, at)?;
                // A gatherer's or a hacker's module is an AI module, whose turret combat reads.
                let inner = match module {
                    SupplyModule::Gatherer | SupplyModule::Hacker => {
                        open.object.fight.field(FightBlock::Ai, word, at)?
                    }
                    SupplyModule::Dock | SupplyModule::Spawn | SupplyModule::Deposit => None,
                };
                match inner {
                    Some(inner) => self.stack.push(Block::Fight(inner)),
                    None if ObjectFiles::opens(word, tokens) => self.stack.push(Block::Other),
                    None => {}
                }
            }
            Block::Object if !self.object.as_ref().is_some_and(|open| open.reskin) => {
                let open = self
                    .object
                    .as_mut()
                    .expect("an object block holds an object");
                let build = &mut open.object.build;
                // A reskin's table sets none of these, so it keeps its base's.
                if build.field(word, at)? {
                    return Ok(());
                }
                let module = tokens.get(1).map(|module| module.to_ascii_lowercase());
                if word == "behavior"
                    && let Some(module) = module
                        .as_deref()
                        .and_then(|module| open.object.supply.module(module))
                {
                    self.stack.push(Block::Supply(module));
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = module
                        .as_deref()
                        .and_then(|module| open.object.powers.module(module))
                {
                    self.stack.push(Block::Power(module));
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = tokens
                        .get(1)
                        .and_then(|class| IniUpgradeModule::module(class))
                {
                    open.object.upgrades.push(module);
                    self.stack.push(Block::Upgrade);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(block) = module.as_deref().and_then(|module| match module {
                        "physicsbehavior" => Some(Block::Physics),
                        "lifetimeupdate" => Some(Block::Lifetime),
                        "grantupgradecreate" => Some(Block::GrantUpgrade),
                        "veterancygaincreate" => {
                            open.object.experience.starts.push(IniStart::default());
                            Some(Block::VeterancyGain)
                        }
                        _ => None,
                    })
                {
                    self.stack.push(block);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(contain) = tokens.get(1).and_then(|class| IniContain::module(class))
                {
                    open.object.contain = Some(contain);
                    self.stack.push(Block::Contain);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = tokens.get(1).and_then(|class| IniDeath::module(class))
                {
                    open.object.deaths.push(module);
                    self.stack.push(Block::Death);
                    return Ok(());
                }
                if let Some(block) = open.object.fight.open(word, tokens) {
                    self.stack.push(Block::Fight(block));
                    return Ok(());
                }
                if open.object.voices.field(word, at)? {
                    return Ok(());
                }
                if word == "unitspecificsounds" && tokens.len() == 1 {
                    self.stack.push(Block::UnitSounds);
                    return Ok(());
                }
                if word == "prerequisites" && tokens.len() == 1 {
                    // `parsePrerequisites` starts the list again.
                    build.prerequisites.clear();
                    self.stack.push(Block::Prerequisites);
                    return Ok(());
                }
                if word == "behavior"
                    && let Some(module) = module.and_then(|module| build.module(&module))
                {
                    self.stack.push(Block::Production(module));
                    return Ok(());
                }
                self.object_field(word, at)?;
            }
            Block::Contain => {
                let open = self.object.as_mut().expect("a module is an object's");
                let contain = open.object.contain.as_mut();
                contain
                    .expect("a contain block opened its module")
                    .field(word, at)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::UnitSounds => {
                let open = self.object.as_mut().expect("a block is an object's");
                open.object.voices.unit_specific(word, at)?;
            }
            Block::Object => self.object_field(word, at)?,
            Block::Other if ObjectFiles::opens(word, tokens) => self.stack.push(Block::Other),
            Block::Other => {}
        }
        Ok(())
    }

    /// A line of an object's own block: a field of its kinds, fence, scale, bridge, locomotors,
    /// sight or geometry, or a line that opens a block.
    fn object_field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let (tokens, line) = (at.tokens, at.number);
        match word {
            _ if matches!(
                word,
                "kindof"
                    | "fencewidth"
                    | "scale"
                    | "isbridge"
                    | "locomotor"
                    | "visionrange"
                    | "shroudclearingrange"
                    | "radarpriority"
                    | "experiencevalue"
                    | "experiencerequired"
                    | "istrainable"
                    | "transportslotcount"
                    | "buttonimage"
            ) || word.starts_with("geometry") =>
            {
                let open = self
                    .object
                    .as_mut()
                    .expect("an object block holds an object");
                let object = &mut open.object;
                // A reskin's table sets its fence's width and its geometry; the game's release
                // build skips a field the table lacks, as its kinds, its scale, whether it is a
                // bridge and its locomotors.
                if object.geometry.field(word, at)? {
                    return Ok(());
                }
                match word {
                    "fencewidth" => object.fence_width = at.real()?,
                    _ if open.reskin => {}
                    "kindof" => object.kinds.set_by(at)?,
                    "isbridge" => object.is_bridge = at.yes()?,
                    "transportslotcount" => object.transport_slots = at.int()?,
                    "buttonimage" => object.button_image = Some(at.value()?.to_owned()),
                    "visionrange" => object.vision_range = at.real()?,
                    "experiencevalue" | "experiencerequired" | "istrainable" => {
                        object.experience.field(word, at)?;
                    }
                    "shroudclearingrange" => object.shroud_clearing_range = at.real()?,
                    "radarpriority" => {
                        object.radar_priority = match at.value()?.to_ascii_uppercase().as_str() {
                            "INVALID" => RadarPriority::Unset,
                            "NOT_ON_RADAR" => RadarPriority::NotOnRadar,
                            "LOCAL_UNIT_ONLY" => RadarPriority::LocalUnitOnly,
                            _ => RadarPriority::Shown,
                        };
                    }
                    "locomotor" => {
                        let set = at.value()?;
                        // `scanIndexList` finds a set of either case.
                        let names = tokens[2..]
                            .iter()
                            .filter(|name| !name.eq_ignore_ascii_case("none"))
                            .map(|&name| name.to_owned());
                        if set.eq_ignore_ascii_case(SET_NORMAL) {
                            object.locomotors = names.collect();
                        } else if set.eq_ignore_ascii_case(SET_TAXIING) {
                            object.taxi_locomotors = names.collect();
                        }
                    }
                    "scale" => {
                        let scale = ModelScale::new(at.real()?);
                        object.scale = scale.ok_or(IniError::Scale { line })?;
                    }
                    _ => {}
                }
            }
            _ if ObjectFiles::opens(word, tokens) => self.stack.push(Block::Other),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod samples;

#[cfg(test)]
mod tests;
