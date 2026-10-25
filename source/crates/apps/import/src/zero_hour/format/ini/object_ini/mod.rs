use std::collections::{BTreeMap, BTreeSet};
use std::ops::Index;

use campfire_client_files::ModelScale;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{FlagList, FlagSet, IniLine, IniText};
use crate::zero_hour::format::ini::object_ini::build_variations::BuildVariations;
use crate::zero_hour::format::ini::object_ini::fight_data::{FightBlock, FightData};
use crate::zero_hour::format::ini::object_ini::ini_build::{IniBuild, ProductionModule};
use crate::zero_hour::format::ini::object_ini::ini_supply::{IniSupply, SupplyModule};
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::IniUpgradeModule;
use crate::zero_hour::format::look_conditions::LookConditions;

pub(crate) mod build_variations;
pub(crate) mod fight_data;
pub(crate) mod ini_build;
pub(crate) mod ini_supply;
pub(crate) mod ini_upgrade_module;

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
    /// The locomotors of its `SET_NORMAL`, in the order its line names them.
    locomotors: Vec<String>,
    build: IniBuild,
    supply: IniSupply,
    fight: FightData,
    upgrades: Vec<IniUpgradeModule>,
}

/// The kinds of an object's `KindOf` that the import reads: those that decide who draws its placed
/// objects (`GameLogic::startNewGame`), an optimized tree and a prop the client alone draws, an
/// object a build clears, which is fluff, the client's too, unless it is a fence, and a wall a unit
/// walks on top of, which the game spawns with the bridges; a structure, which routes block when it
/// is not small; an object that counts for a multiplayer victory, which a structure of it keeps
/// its player in the match by (09-zero-hour.md, Defeat by the last building that counts); a base
/// defense, which a human player's idle unit takes as a target among structures; infantry,
/// which a weapon may scatter more at; and a vehicle, which drives through a crowd as one that
/// turns, not one that steps (04-capabilities/navigation.md, A unit moves by its kind of motion).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObjectKinds {
    optimized_tree: bool,
    prop: bool,
    cleared_by_build: bool,
    walk_on_top_of_wall: bool,
    structure: bool,
    mp_count_for_victory: bool,
    fs_base_defense: bool,
    infantry: bool,
    vehicle: bool,
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
/// state's look, and the bone its turret turns, its `Turret`; the sets of flags its default state
/// is drawn for, none and its aliases'; its other condition states, in order; the map conditions
/// it ignores (`IgnoreConditionStates`); and every animation its states and transitions name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DrawModule {
    pub(crate) kind: DrawKind,
    /// Its default state's model, in lowercase.
    pub(crate) model: Option<String>,
    pub(crate) look: StateLook,
    pub(crate) turret: Option<String>,
    sets: Vec<ConditionSet>,
    states: Vec<DrawState>,
    ignored: ConditionSet,
    /// Each by its name, `<hierarchy>.<animation>` in lowercase, as `parseAnimation` keeps it.
    pub(crate) animations: BTreeSet<String>,
}

/// A condition state of a `W3DModelDraw` past its default: the sets of flags it is drawn for, its
/// own and its aliases', and its model and look, which start as the default state's when the
/// module has a `DefaultConditionState`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DrawState {
    sets: Vec<ConditionSet>,
    model: Option<String>,
    look: StateLook,
}

/// A set of a state's condition flags, each in lowercase, in order and once.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ConditionSet {
    flags: Vec<String>,
}

/// A state of a draw module, its default or a later one: the sets of flags it is drawn for, its
/// model and its look.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ModuleState<'a> {
    pub(crate) sets: &'a [ConditionSet],
    pub(crate) drawn: DrawnState<'a>,
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

/// A weapon slot's bones, each a base name the game numbers from 01.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WeaponBones {
    pub(crate) fire_fx: Option<String>,
    pub(crate) recoil: Option<String>,
    pub(crate) muzzle_flash: Option<String>,
    pub(crate) launch: Option<String>,
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

/// The kinds of `KindOf` the import reads, as the game names them.
const OPTIMIZED_TREE: &str = "optimized_tree";
const PROP: &str = "prop";
const CLEARED_BY_BUILD: &str = "cleared_by_build";
const WALK_ON_TOP_OF_WALL: &str = "walk_on_top_of_wall";
const STRUCTURE: &str = "structure";
const MP_COUNT_FOR_VICTORY: &str = "mp_count_for_victory";
const FS_BASE_DEFENSE: &str = "fs_base_defense";
const INFANTRY: &str = "infantry";
const VEHICLE: &str = "vehicle";

/// The locomotor set an object moves by, of `TheLocomotorSetNames`.
const SET_NORMAL: &str = "set_normal";

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
    /// A `TransitionState` of such a module, which the look reads the animations of.
    Transition,
    /// An object's `Prerequisites`.
    Prerequisites,
    /// A block combat reads: a body, a weapon set or an armor set.
    Fight(FightBlock),
    /// A behavior module whose fields production reads.
    Production(ProductionModule),
    /// A behavior module whose fields supply reads.
    Supply(SupplyModule),
    /// An upgrade module, the object's last.
    Upgrade,
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
                build: IniBuild::default(),
                supply: IniSupply::default(),
                fight: FightData::default(),
                upgrades: Vec::new(),
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

impl ObjectKinds {
    /// Sets the kinds by a `KindOf` line's flags, as `BitFlags::parse` does; a kind the import
    /// does not read changes nothing.
    fn set_by(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        at.set_flags(self)
    }

    /// Sets or clears the kind `name`, of either case.
    fn set_kind(&mut self, name: &str, set: bool) {
        let kind = match name.to_ascii_lowercase().as_str() {
            OPTIMIZED_TREE => &mut self.optimized_tree,
            PROP => &mut self.prop,
            CLEARED_BY_BUILD => &mut self.cleared_by_build,
            WALK_ON_TOP_OF_WALL => &mut self.walk_on_top_of_wall,
            STRUCTURE => &mut self.structure,
            MP_COUNT_FOR_VICTORY => &mut self.mp_count_for_victory,
            FS_BASE_DEFENSE => &mut self.fs_base_defense,
            INFANTRY => &mut self.infantry,
            VEHICLE => &mut self.vehicle,
            _ => return,
        };
        *kind = set;
    }
}

impl FlagSet for ObjectKinds {
    fn clear(&mut self) {
        *self = Self::default();
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        self.set_kind(name, on);
        true
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
            sets: vec![ConditionSet::default()],
            states: Vec::new(),
            ignored: ConditionSet::default(),
            animations: BTreeSet::new(),
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
        let held = |flag: &String| held(flag) && !self.ignored.flags.contains(flag);
        let mut best = None;
        let (mut most, mut fewest) = (0, usize::MAX);
        for state in self.states() {
            for set in state.sets {
                let common = set.flags.iter().filter(|flag| held(flag)).count();
                let lacked = set.flags.len() - common;
                if common > most || (common >= most && lacked < fewest) {
                    (best, most, fewest) = (Some(state.drawn), common, lacked);
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
        };
        let states = self.states.iter().map(|state| ModuleState {
            sets: &state.sets,
            drawn: DrawnState {
                model: state.model.as_deref(),
                look: &state.look,
            },
        });
        std::iter::once(default).chain(states)
    }

    /// The flags it ignores as it matches its states, `IgnoreConditionStates`.
    pub(crate) fn ignored(&self) -> &[String] {
        &self.ignored.flags
    }

    /// A field of its default state that the look reads.
    fn default_field(&mut self, word: &str, tokens: &[&str], line: usize) -> Result<(), IniError> {
        if word == "turret"
            && let Some(bone) = tokens.get(1)
        {
            self.turret = Some((*bone).to_owned());
        }
        state_field(&mut self.model, &mut self.look, word, tokens, line)
    }

    /// A field of a state or a transition that names an animation, `Animation` or
    /// `IdleAnimation`: its name, of none for `NONE` or no name, as `parseAnimation` reads it.
    fn animation_field(&mut self, word: &str, tokens: &[&str]) {
        if matches!(word, "animation" | "idleanimation")
            && let Some(name) = tokens.get(1)
            && !name.eq_ignore_ascii_case("none")
        {
            self.animations.insert(name.to_ascii_lowercase());
        }
    }

    /// The state the reader is in past the default, its last.
    fn last_state(&mut self) -> &mut DrawState {
        self.states
            .last_mut()
            .expect("a state's block is in its module")
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
        "weaponfirefxbone" | "weaponrecoilbone" | "weaponmuzzleflash" | "weaponlaunchbone" => {
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
        if self.is_bridge || kinds.walk_on_top_of_wall {
            PlacedAs::Bridge
        } else if kinds.optimized_tree {
            PlacedAs::Tree
        } else if kinds.prop || (kinds.cleared_by_build && self.fence_width == 0.0) {
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

    /// Its upgrade modules, in the order it gives them.
    pub(crate) fn upgrades(&self) -> &[IniUpgradeModule] {
        &self.upgrades
    }

    /// Whether it counts for a multiplayer victory: a structure of the kind
    /// `MP_COUNT_FOR_VICTORY`, as `Team::hasAnyBuildings` counts it.
    pub(crate) const fn counts_for_victory(&self) -> bool {
        self.kinds.structure && self.kinds.mp_count_for_victory
    }

    /// Whether it is a structure, of `KindOf`.
    pub(crate) const fn is_structure(&self) -> bool {
        self.kinds.structure
    }

    /// Whether it is a base defense, of `KindOf`'s `FS_BASE_DEFENSE`.
    pub(crate) const fn is_base_defense(&self) -> bool {
        self.kinds.fs_base_defense
    }

    /// Whether it is infantry, of `KindOf`.
    pub(crate) const fn is_infantry(&self) -> bool {
        self.kinds.infantry
    }

    /// Whether it is a vehicle, of `KindOf`.
    pub(crate) const fn is_vehicle(&self) -> bool {
        self.kinds.vehicle
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
            Block::DefaultState { named: false } | Block::State { named: false } => {
                Err(IniError::NoModel { line })
            }
            Block::DefaultState { named: true }
            | Block::State { named: true }
            | Block::Transition
            | Block::Prerequisites
            | Block::Fight(_)
            | Block::Production(_)
            | Block::Supply(_)
            | Block::Upgrade
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
                            };
                            if *copied {
                                state.model.clone_from(&module.model);
                                state.look.clone_from(&module.look);
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
                        self.stack.push(Block::Transition);
                    }
                    "ignoreconditionstates" if *kind == DrawKind::Family => {
                        module.ignored = ConditionSet::of(at)?;
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
                module.animation_field(word, tokens);
                let state = module.last_state();
                state_field(&mut state.model, &mut state.look, word, tokens, line)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::DefaultState { named } => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                module.animation_field(word, tokens);
                module.default_field(word, tokens, line)?;
                if ObjectFiles::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Transition => {
                let module = self.draw.as_mut().expect("a transition is in a draw");
                module.animation_field(word, tokens);
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
                self.draw = Some(DrawModule::new(kind));
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
                    && let Some(module) = tokens
                        .get(1)
                        .and_then(|class| IniUpgradeModule::module(class))
                {
                    open.object.upgrades.push(module);
                    self.stack.push(Block::Upgrade);
                    return Ok(());
                }
                if let Some(block) = open.object.fight.open(word, tokens) {
                    self.stack.push(Block::Fight(block));
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
            Block::Object => self.object_field(word, at)?,
            Block::Other if ObjectFiles::opens(word, tokens) => self.stack.push(Block::Other),
            Block::Other => {}
        }
        Ok(())
    }

    /// A line of an object's own block: a field of its kinds, fence, scale, bridge, locomotors
    /// or geometry, or a line that opens a block.
    fn object_field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let (tokens, line) = (at.tokens, at.number);
        match word {
            _ if matches!(
                word,
                "kindof" | "fencewidth" | "scale" | "isbridge" | "locomotor"
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
                    "locomotor" => {
                        let set = at.value()?;
                        // `scanIndexList` finds a set of either case.
                        if set.eq_ignore_ascii_case(SET_NORMAL) {
                            let names = tokens[2..]
                                .iter()
                                .filter(|name| !name.eq_ignore_ascii_case("none"));
                            object.locomotors = names.map(|&name| name.to_owned()).collect();
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
