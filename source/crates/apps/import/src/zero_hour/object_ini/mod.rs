use std::collections::BTreeMap;

use campfire_client_files::ModelScale;

use crate::zero_hour::error::IniError;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::ini_text::{FlagList, IniLine, IniText};

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
}

/// The kinds of an object's `KindOf` that the import reads: those that decide who draws its
/// placed objects (`GameLogic::startNewGame`), an optimized tree and a prop the client alone
/// draws, an object a build clears, which is fluff, the client's too, unless it is a fence, and a
/// wall a unit walks on top of, which the game spawns with the bridges; and a structure, which
/// routes block when it is not small.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObjectKinds {
    optimized_tree: bool,
    prop: bool,
    cleared_by_build: bool,
    walk_on_top_of_wall: bool,
    structure: bool,
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
/// that state's look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DrawModule {
    pub(crate) kind: DrawKind,
    pub(crate) model: Option<String>,
    pub(crate) look: DefaultLook,
}

/// What a default state hides and shows, and the bones of its weapons, by slot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DefaultLook {
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
const SLOTS: [&str; 3] = ["primary", "secondary", "tertiary"];

/// A block the reader is inside.
#[derive(Debug)]
enum Block {
    Object,
    /// A draw module, with the count of its states begun.
    Draw {
        kind: DrawKind,
        states: u32,
    },
    /// The first state of such a module, its default, and whether it named its model.
    DefaultState {
        named: bool,
    },
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
    /// A field of a default state that the look reads.
    fn default_field(&mut self, word: &str, tokens: &[&str], line: usize) -> Result<(), IniError> {
        match word {
            "model" => self.model = ObjectIni::model(tokens),
            "hidesubobject" | "showsubobject" => {
                self.look.hide_show(&tokens[1..], word == "hidesubobject");
            }
            "weaponfirefxbone" | "weaponrecoilbone" | "weaponmuzzleflash" | "weaponlaunchbone" => {
                let (Some(slot), Some(bone)) = (tokens.get(1), tokens.get(2)) else {
                    return Err(IniError::WeaponSlot { line });
                };
                let slot = SLOTS
                    .iter()
                    .position(|known| slot.eq_ignore_ascii_case(known))
                    .ok_or(IniError::WeaponSlot { line })?;
                let bones = &mut self.look.weapons[slot];
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
}

impl DefaultLook {
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
            Block::DefaultState { named: false } => Err(IniError::NoModel { line }),
            Block::DefaultState { named: true } | Block::Other => Ok(None),
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
            Block::Draw { kind, states } => {
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
                        self.stack.push(if default {
                            Block::DefaultState { named: false }
                        } else {
                            Block::Other
                        });
                    }
                    "modelname" if matches!(*kind, DrawKind::Tree | DrawKind::Prop) => {
                        module.model = ObjectIni::model(tokens);
                    }
                    _ if ObjectIni::opens(word, tokens) => self.stack.push(Block::Other),
                    _ => {}
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
                    .map(|kind| kind.to_ascii_lowercase())
                    .unwrap_or_default();
                let kind = match module.as_str() {
                    family if MODEL_DRAWS.contains(&family) => DrawKind::Family,
                    "w3dtreedraw" => DrawKind::Tree,
                    "w3dpropdraw" => DrawKind::Prop,
                    _ => DrawKind::Other,
                };
                self.draw = Some(DrawModule {
                    kind,
                    model: None,
                    look: DefaultLook::default(),
                });
                self.stack.push(Block::Draw { kind, states: 0 });
            }
            Block::Object
                if matches!(
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
    /// extents 4 and 3, and its fence.
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
  Locomotor = SET_NORMAL TankTreads\r
  Geometry = BOX\r
  GeometryMajorRadius = 4.0\r
  GeometryMinorRadius = 3.0\r
End\r
Object FortressWall\r
  KindOf = WALK_ON_TOP_OF_WALL IMMOBILE STRUCTURE\r
  Geometry = BOX\r
  GeometryMajorRadius = 10.0\r
  GeometryMinorRadius = 2.5\r
End\r
Object Rocks1\r
  KindOf = CLEARED_BY_BUILD\r
  FenceWidth = 10.0\r
End\r
";
}

#[cfg(test)]
mod tests;
