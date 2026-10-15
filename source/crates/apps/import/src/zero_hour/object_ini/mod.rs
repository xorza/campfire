use std::collections::BTreeMap;

use campfire_client_files::ModelScale;

use crate::zero_hour::error::IniError;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::ini_text::{FlagList, IniLine, IniText};
use crate::zero_hour::look_conditions::LookConditions;
use crate::zero_hour::object_ini::build_data::{BuildData, ProductionModule};
use crate::zero_hour::object_ini::fight_data::{FightBlock, FightData};

pub(crate) mod build_data;
pub(crate) mod fight_data;

/// The objects of Zero Hour's object INI files, as `ThingFactory` reads them, each with what its
/// draw modules show by default.
#[derive(Debug, Default)]
pub(crate) struct ObjectIni {
    /// Each object by its name, of its case, as `ThingFactory::findTemplate` finds it.
    objects: BTreeMap<String, IniObject>,
}

/// An object as the game holds it once read, a reskin a copy of its base with its own fields
/// over it (`ThingTemplate::copyFrom`): its name as the INI writes it, its draw modules, and the
/// fields that say how the game draws, places and moves it: its kinds, its fence's width, its
/// asset scale, whether it is a landmark bridge, its geometry and its normal locomotors by name,
/// each the game's default when absent.
#[derive(Debug, Clone)]
pub(crate) struct IniObject {
    pub(crate) name: String,
    draws: Vec<DrawModule>,
    kinds: ObjectKinds,
    fence_width: f32,
    scale: ModelScale,
    is_bridge: bool,
    geometry: Geometry,
    /// The locomotors of its `SET_NORMAL`, in the order its line names them.
    locomotors: Vec<String>,
    build: BuildData,
    fight: FightData,
}

/// The kinds of an object's `KindOf` that the import reads: those that decide who draws its
/// placed objects (`GameLogic::startNewGame`), an optimized tree and a prop the client alone
/// draws, an object a build clears, which is fluff, the client's too, unless it is a fence, and a
/// wall a unit walks on top of, which the game spawns with the bridges; a structure, which
/// routes block when it is not small; and an object that counts for a multiplayer victory, which
/// a structure of it keeps its player in the match by (design 19, D7).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObjectKinds {
    optimized_tree: bool,
    prop: bool,
    cleared_by_build: bool,
    walk_on_top_of_wall: bool,
    structure: bool,
    mp_count_for_victory: bool,
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

/// Who draws an object a map places, as a multiplayer game spawns it (design 12, D14): a unit of
/// the sim placed before every other, as the game spawns bridge-like objects in a pass of their
/// own; the client's tree buffer; its prop buffer; or the sim, as a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacedAs {
    Bridge,
    Tree,
    Prop,
    Unit,
}

/// A draw module: how it names its model, the model of its default state, if it draws one, and
/// that state's look; the sets of flags its default state is drawn for, none and its aliases';
/// its other condition states, in order; and the map conditions it ignores
/// (`IgnoreConditionStates`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DrawModule {
    pub(crate) kind: DrawKind,
    pub(crate) model: Option<String>,
    pub(crate) look: StateLook,
    sets: Vec<ConditionSet>,
    states: Vec<DrawState>,
    ignored: LookConditions,
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

/// A set of a state's condition flags as the import counts them: whether it holds `NIGHT` and
/// `SNOW`, and how many other flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ConditionSet {
    conditions: LookConditions,
    others: usize,
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
/// `Turret = TurretBone` is.
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
    /// An object's `Prerequisites`.
    Prerequisites,
    /// A block combat reads: a body, a weapon set or an armor set.
    Fight(FightBlock),
    /// A behavior module whose fields production reads.
    Production(ProductionModule),
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

impl ObjectIni {
    /// Reads one INI file, adding its objects to those read before. An object of a name read
    /// before, and a reskin of a base not yet read, are refused: no known install holds either,
    /// where the game's release build parses the second definition into the first, and refuses
    /// a reskin of no base.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut reading = Reading::default();
        for line in text.lines() {
            let word = line.word();
            if word == "end" {
                if let Some(done) = reading.end(line.number)? {
                    self.objects.insert(done.name.clone(), done);
                }
            } else if reading.stack.is_empty() {
                reading.open(self.object_of(&word, &line)?);
            } else {
                reading.line(&word, &line)?;
            }
        }
        if reading.stack.is_empty() {
            Ok(())
        } else {
            Err(IniError::Unclosed)
        }
    }

    /// Every object, in the order of their names.
    pub(crate) fn objects(&self) -> impl Iterator<Item = &IniObject> {
        self.objects.values()
    }

    /// The object a map's template names, of its case, as `findTemplate` finds it; none for a
    /// template no INI object names, which the game spawns nothing of.
    pub(crate) fn object(&self, template: &[u8]) -> Option<&IniObject> {
        let name: String = template.iter().map(|&byte| char::from(byte)).collect();
        self.objects.get(&name)
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
        if self.objects.contains_key(name) {
            return Err(IniError::Redefined { line });
        }
        let object = match base {
            None => IniObject {
                name: name.to_owned(),
                draws: Vec::new(),
                kinds: ObjectKinds::default(),
                fence_width: 0.0,
                scale: ModelScale::ONE,
                is_bridge: false,
                geometry: Geometry::DEFAULT,
                locomotors: Vec::new(),
                build: BuildData::default(),
                fight: FightData::default(),
            },
            Some(base) => IniObject {
                name: name.to_owned(),
                ..self
                    .objects
                    .get(base)
                    .ok_or(IniError::NoBase { line })?
                    .clone()
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

    /// A model field's value: none for `NONE`.
    fn model(tokens: &[&str]) -> Option<String> {
        tokens
            .get(1)
            .filter(|model| !model.eq_ignore_ascii_case("none"))
            .map(|model| (*model).to_owned())
    }
}

impl ObjectKinds {
    /// Sets the kinds by a `KindOf` line's flags, as `BitFlags::parse` does; a kind the import
    /// does not read changes nothing.
    fn set_by(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        match at.flags()? {
            FlagList::Clear => *self = ObjectKinds::default(),
            FlagList::Only(names) => {
                *self = ObjectKinds::default();
                for name in names {
                    self.set(name, true);
                }
            }
            FlagList::Change(changes) => {
                for change in changes {
                    self.set(change.name, change.set);
                }
            }
        }
        Ok(())
    }

    /// Sets or clears the kind `name`, of either case.
    fn set(&mut self, name: &str, set: bool) {
        let kind = match name.to_ascii_lowercase().as_str() {
            OPTIMIZED_TREE => &mut self.optimized_tree,
            PROP => &mut self.prop,
            CLEARED_BY_BUILD => &mut self.cleared_by_build,
            WALK_ON_TOP_OF_WALL => &mut self.walk_on_top_of_wall,
            STRUCTURE => &mut self.structure,
            MP_COUNT_FOR_VICTORY => &mut self.mp_count_for_victory,
            _ => return,
        };
        *kind = set;
    }
}

impl Geometry {
    /// A thing's geometry until its fields say otherwise: a sphere of radius 1, not small.
    const DEFAULT: Geometry = Geometry {
        shape: GeometryShape::Sphere,
        major: 1.0,
        minor: 1.0,
        height: 1.0,
        small: false,
    };

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
    fn new(kind: DrawKind) -> DrawModule {
        DrawModule {
            kind,
            model: None,
            look: StateLook::default(),
            sets: vec![ConditionSet::default()],
            states: Vec::new(),
            ignored: LookConditions::default(),
        }
    }

    /// The model and the look it draws on a map of `conditions`, those it ignores dropped: of
    /// its state whose set of flags best matches them, as `SparseMatchFinder::findBestInfoSlow`
    /// finds it: the set of the most flags in common, of a tie the one of the fewest the map
    /// lacks, of a tie the first, the states in order, the default first.
    pub(crate) fn drawn(&self, conditions: LookConditions) -> DrawnState<'_> {
        let map = LookConditions {
            night: conditions.night && !self.ignored.night,
            snow: conditions.snow && !self.ignored.snow,
        };
        let default = DrawnState {
            model: self.model.as_deref(),
            look: &self.look,
        };
        let mut best = default;
        let (mut most, mut fewest) = (0, usize::MAX);
        let states = self.states.iter().map(|state| {
            let drawn = DrawnState {
                model: state.model.as_deref(),
                look: &state.look,
            };
            (&state.sets, drawn)
        });
        for (sets, drawn) in std::iter::once((&self.sets, default)).chain(states) {
            for set in sets {
                let [night, snow] = [set.conditions.night, set.conditions.snow];
                let common = usize::from(night && map.night) + usize::from(snow && map.snow);
                let lacked =
                    usize::from(night && !map.night) + usize::from(snow && !map.snow) + set.others;
                if common > most || (common >= most && lacked < fewest) {
                    (best, most, fewest) = (drawn, common, lacked);
                }
            }
        }
        best
    }

    /// A field of its default state that the look reads.
    fn default_field(&mut self, word: &str, tokens: &[&str], line: usize) -> Result<(), IniError> {
        state_field(&mut self.model, &mut self.look, word, tokens, line)
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
    fn of(at: &IniLine<'_>) -> Result<ConditionSet, IniError> {
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
        let mut set = ConditionSet::default();
        for flag in &flags {
            match flag.as_str() {
                "night" => set.conditions.night = true,
                "snow" => set.conditions.snow = true,
                _ => set.others += 1,
            }
        }
        Ok(set)
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
        "model" => *model = ObjectIni::model(tokens),
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
    pub(crate) const fn build(&self) -> &BuildData {
        &self.build
    }

    /// What it gives combat: its body, its weapon sets and its armor sets.
    pub(crate) const fn fight(&self) -> &FightData {
        &self.fight
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

    /// The asset scale every drawable of it takes: its `Scale`.
    pub(crate) const fn scale(&self) -> ModelScale {
        self.scale
    }

    /// The name a package gives it, of its name's bytes, each char of Latin-1 one, as a map's
    /// template writes it.
    pub(crate) fn import_name(&self) -> ImportName {
        let bytes: Vec<u8> = self
            .name
            .chars()
            .map(|char| u8::try_from(char).expect("an INI's text is Latin-1"))
            .collect();
        ImportName::of(&bytes)
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
            | Block::Prerequisites
            | Block::Fight(_)
            | Block::Production(_)
            | Block::Other => Ok(None),
        }
    }

    /// A line other than `End`, of its first word in lowercase, `word`.
    fn line(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let (tokens, line) = (&*at.tokens, at.number);
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
                    "ignoreconditionstates" if *kind == DrawKind::Family => {
                        module.ignored = ConditionSet::of(at)?.conditions;
                    }
                    "modelname" if matches!(*kind, DrawKind::Tree | DrawKind::Prop) => {
                        module.model = ObjectIni::model(tokens);
                    }
                    _ if ObjectIni::opens(word, tokens) => self.stack.push(Block::Other),
                    _ => {}
                }
            }
            Block::State { named } => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                let state = module.last_state();
                state_field(&mut state.model, &mut state.look, word, tokens, line)?;
                if ObjectIni::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::DefaultState { named } => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                module.default_field(word, tokens, line)?;
                if ObjectIni::opens(word, tokens) {
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
                open.object.fight.field(block, word, at)?;
                if ObjectIni::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Block::Production(module) => {
                let module = *module;
                let open = self.object.as_mut().expect("a module is an object's");
                open.object.build.module_field(module, word, at)?;
                if ObjectIni::opens(word, tokens) {
                    self.stack.push(Block::Other);
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
                let module = tokens.get(1).map(|module| module.to_ascii_lowercase());
                if word == "behavior"
                    && let Some(module) = module.and_then(|module| build.module(&module))
                {
                    self.stack.push(Block::Production(module));
                    return Ok(());
                }
                self.object_field(word, at)?;
            }
            Block::Object => self.object_field(word, at)?,
            Block::Other if ObjectIni::opens(word, tokens) => self.stack.push(Block::Other),
            Block::Other => {}
        }
        Ok(())
    }

    /// A line of an object's own block: a field of its kinds, fence, scale, bridge, locomotors
    /// or geometry, or a line that opens a block.
    fn object_field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let (tokens, line) = (&*at.tokens, at.number);
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
            _ if ObjectIni::opens(word, tokens) => self.stack.push(Block::Other),
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// `Data\INI\Default\Object.ini` of the fixture install: a tank of two draw modules, one of
    /// which draws no model, and a tree.
    pub(crate) const DEFAULT_OBJECTS: &str = "; Objects the game loads first.\r
Object Tank\r
  Draw = W3DTankDraw ModuleTag_01\r
    DefaultConditionState\r
      Model = TANK\r
      HideSubObject = Turret\r
      ShowSubObject = Barrel\r
      HideSubObject = barrel Hatch\r
      WeaponMuzzleFlash = PRIMARY MuzzleFX\r
      WeaponLaunchBone = PRIMARY None\r
      WeaponFireFXBone = SECONDARY Fire ; a slot's bones need not name a flash\r
      Turret = TurretBone\r
    End\r
    ConditionState = REALLYDAMAGED\r
      Model = TANK_D\r
    End\r
    ConditionState = NIGHT\r
      ShowSubObject = Turret\r
    End\r
  End\r
  Draw = W3DDebrisDraw ModuleTag_02\r
    Model = Debris\r
  End\r
  Behavior = SlowDeathBehavior ModuleTag_03\r
    Turret\r
    End\r
  End\r
  ArmorSet\r
    Conditions = None\r
  End\r
End\r
\r
Object Tree\r
  Draw W3DTreeDraw ModuleTag_01\r
    ModelName PTOak01\r
  End\r
End\r
";

    /// `Data\INI\Object\Misc.ini` of the fixture install: objects of models that draw nothing,
    /// of an emitter, and of no file.
    pub(crate) const MORE_OBJECTS: &str = "Object Crate
  Draw = W3DModelDraw ModuleTag_01
    DefaultConditionState
      Model = Prop
    End
  End
End
Object Marker
  Draw = W3DModelDraw ModuleTag_01
    DefaultConditionState
      Model = NULL
    End
  End
End
Object Hydrant
  Draw = W3DModelDraw ModuleTag_01
    DefaultConditionState
      Model = Spray
    End
  End
End
Object Ghost
  KindOf = PROP
  Draw = W3DModelDraw ModuleTag_01
    DefaultConditionState
      Model = Absent
    End
  End
End
";

    /// `Data\INI\Object.ini` of the fixture install: two reskins of the tank, one with its own
    /// draw module, a stump of no model, a prop of a lone mesh at a scale of 1.5 and its
    /// reskin, an optimized tree, fluff, a wall a unit walks on, a structure of a box of half
    /// extents 10 and 2.5, and the fixture map's tank, which drives on its treads, a box of half
    /// extents 4 and 3, and its fence; and America's start objects, which no map places: a
    /// command center of a box of half extents 12 and 9 and height 40, whose sphere's radius is
    /// √(144 + 81 + 400) = 25, and a dozer on treads.
    pub(crate) const OBJECTS: &str = "ObjectReskin TankCamo Tank\r
End\r
ObjectReskin TankRed Tank\r
  Draw = W3DModelDraw ModuleTag_01\r
    ConditionState NONE\r
      Model = TANK\r
      HideSubObject = Barrel\r
      ShowSubObject = None\r
      ShowSubObject = Hatch\r
    End\r
  End\r
End\r
Object Stump\r
  Draw = W3DPropDraw ModuleTag_01\r
    ModelName = NONE\r
  End\r
End\r
Object Rock\r
  KindOf = PROP\r
  Scale = 1.5\r
  Draw = W3DModelDraw ModuleTag_01\r
    ConditionState\r
      Model = Rock01\r
    End\r
  End\r
End\r
ObjectReskin RockRed Rock\r
  Scale = 3.0\r
End\r
Object Oak\r
  KindOf = OPTIMIZED_TREE\r
  Draw = W3DModelDraw ModuleTag_01\r
    DefaultConditionState\r
      Model = TANK\r
    End\r
  End\r
  Draw = W3DTreeDraw ModuleTag_02\r
    ModelName = Rock01\r
  End\r
End\r
Object Bush\r
  KindOf = CLEARED_BY_BUILD\r
  Draw = W3DModelDraw ModuleTag_01\r
    DefaultConditionState\r
      Model = TANK\r
      HideSubObject = Turret\r
    End\r
  End\r
End\r
Object AmericaTankCrusader\r
  KindOf = VEHICLE\r
  Body = ActiveBody ModuleTag_04\r
    MaxHealth = 400.0\r
    InitialHealth = 400.0\r
  End\r
  WeaponSet\r
    Conditions = None\r
    Weapon = PRIMARY TankCannon\r
    Weapon = SECONDARY RangerRifle\r
    Weapon = TERTIARY Railgun\r
  End\r
  Locomotor = SET_NORMAL TankTreads\r
  Geometry = BOX\r
  GeometryMajorRadius = 4.0\r
  GeometryMinorRadius = 3.0\r
End\r
Object FortressWall\r
  KindOf = WALK_ON_TOP_OF_WALL IMMOBILE STRUCTURE\r
  Body = HighlanderBody ModuleTag_05\r
    MaxHealth = 1000.0\r
  End\r
  Geometry = BOX\r
  GeometryMajorRadius = 10.0\r
  GeometryMinorRadius = 2.5\r
End\r
Object Rocks1\r
  KindOf = CLEARED_BY_BUILD\r
  FenceWidth = 10.0\r
End\r
Object AmericaCommandCenter\r
  KindOf = STRUCTURE MP_COUNT_FOR_VICTORY\r
  Body = StructureBody ModuleTag_08\r
    MaxHealth = 5000.0\r
    InitialHealth = 5000.0\r
  End\r
  ArmorSet\r
    Conditions = None\r
    Armor = StructureArmor\r
  End\r
  Geometry = BOX\r
  GeometryMajorRadius = 12.0\r
  GeometryMinorRadius = 9.0\r
  GeometryHeight = 40.0\r
  BuildCost = 2000\r
  BuildTime = 45.0\r
  CommandSet = AmericaCommandCenterCommandSet\r
  PlacementViewAngle = -135\r
  Behavior = ProductionUpdate ModuleTag_11\r
  End\r
  Behavior = DefaultProductionExitUpdate ModuleTag_13\r
    UnitCreatePoint = X:-18.0 Y:35.0 Z:0.0\r
    NaturalRallyPoint = X:60.0 Y:35.0 Z:0.0\r
  End\r
End\r
Object AmericaVehicleDozer\r
  KindOf = VEHICLE\r
  Body = ActiveBody ModuleTag_02\r
    MaxHealth = 250.0\r
    InitialHealth = 200.0\r
  End\r
  Locomotor = SET_NORMAL TankTreads\r
  Geometry = SPHERE\r
  GeometryMajorRadius = 2.0\r
  BuildCost = 1000\r
  BuildTime = 5.0\r
  CommandSet = AmericaDozerCommandSet\r
End\r
Object AmericaBarracks\r
  KindOf = STRUCTURE\r
  Body = StructureBody ModuleTag_09\r
    MaxHealth = 1200.0\r
    InitialHealth = 1200.0\r
  End\r
  Geometry = BOX\r
  GeometryMajorRadius = 5.0\r
  GeometryMinorRadius = 4.0\r
  BuildCost = 600\r
  BuildTime = 10.0\r
  CommandSet = AmericaBarracksCommandSet\r
  Prerequisites\r
  End\r
  Behavior = ProductionUpdate ModuleTag_11\r
    MaxQueueEntries = 5\r
  End\r
  Behavior = QueueProductionExitUpdate ModuleTag_12\r
    UnitCreatePoint = X:0.0 Y:0.0 Z:0.0\r
    NaturalRallyPoint = X:55.0 Y:0.0 Z:0.0\r
  End\r
End\r
Object AmericaInfantryRanger\r
  KindOf = INFANTRY\r
  Body = ActiveBody ModuleTag_03\r
    MaxHealth = 180.0\r
    InitialHealth = 180.0\r
  End\r
  WeaponSet\r
    Conditions = PLAYER_UPGRADE\r
    Weapon = PRIMARY Flamer\r
  End\r
  WeaponSet\r
    Conditions = None\r
    Weapon = PRIMARY RangerRifle\r
    Weapon = SECONDARY Pistol\r
    Weapon = TERTIARY NONE\r
  End\r
  ArmorSet\r
    Conditions = None\r
    Armor = HumanArmor\r
  End\r
  Locomotor = SET_NORMAL TankTreads\r
  Geometry = CYLINDER\r
  GeometryMajorRadius = 1.0\r
  BuildCost = 225\r
  BuildTime = 5.0\r
  Prerequisites\r
    Object = AmericaBarracks\r
  End\r
End\r
Object AmericaInfantryPathfinder\r
  KindOf = INFANTRY\r
  Locomotor = SET_NORMAL TankTreads\r
  Prerequisites\r
    Object = AmericaBarracks AmericaCommandCenter\r
  End\r
End\r
Object AmericaPowerPlant\r
  KindOf = STRUCTURE\r
  Geometry = BOX\r
  GeometryMajorRadius = 3.0\r
  GeometryMinorRadius = 3.0\r
  Prerequisites\r
    Science = SCIENCE_AMERICA\r
  End\r
End\r
Object AmericaFireBase\r
  KindOf = STRUCTURE\r
  GeometryIsSmall = Yes\r
End\r
";
}

#[cfg(test)]
mod tests;
