use std::collections::BTreeMap;

use campfire_client_files::ModelScale;

use crate::zero_hour::error::IniError;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::ini_text::{IniLine, IniText};

/// The objects of Zero Hour's object INI files, as `ThingFactory` reads them, each with what its
/// draw modules show by default.
#[derive(Debug, Default)]
pub(crate) struct ObjectIni {
    /// Each object by its name, of its case, as `ThingFactory::findTemplate` finds it.
    objects: BTreeMap<String, IniObject>,
}

/// An object as the game holds it once read, a reskin a copy of its base with its own fields
/// over it (`ThingTemplate::copyFrom`): its name as the INI writes it, its draw modules, and the
/// fields that say how the game draws and places it: its kinds, its fence's width, its asset
/// scale and whether it is a landmark bridge, each the game's default when absent.
#[derive(Debug, Clone)]
pub(crate) struct IniObject {
    pub(crate) name: String,
    draws: Vec<DrawModule>,
    kinds: ObjectKinds,
    fence_width: f32,
    scale: ModelScale,
    is_bridge: bool,
}

/// The kinds of an object's `KindOf` that decide who draws its placed objects
/// (`GameLogic::startNewGame`): an optimized tree and a prop the client alone draws; an object a
/// build clears, which is fluff, the client's too, unless it is a fence; and a wall a unit walks
/// on top of, which the game spawns with the bridges.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObjectKinds {
    optimized_tree: bool,
    prop: bool,
    cleared_by_build: bool,
    walk_on_top_of_wall: bool,
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
    /// Sets the kinds by a `KindOf` line's flags, as `BitFlags::parse` does: `NONE` clears them;
    /// a list of names sets those it names and clears the rest; `+` and `-` set and clear one,
    /// and keep the rest; a list may not mix the two forms.
    fn set_by(&mut self, flags: &[&str], line: usize) -> Result<(), IniError> {
        let mut plain = false;
        let mut signed = false;
        for flag in flags {
            if flag.eq_ignore_ascii_case("none") {
                if plain || signed {
                    return Err(IniError::MixedKinds { line });
                }
                *self = ObjectKinds::default();
                break;
            }
            let (name, set) = match flag.strip_prefix('+') {
                Some(name) => (name, Some(true)),
                None => match flag.strip_prefix('-') {
                    Some(name) => (name, Some(false)),
                    None => (*flag, None),
                },
            };
            match set {
                Some(_) if plain => return Err(IniError::MixedKinds { line }),
                Some(_) => signed = true,
                None if signed => return Err(IniError::MixedKinds { line }),
                None => {
                    if !plain {
                        *self = ObjectKinds::default();
                    }
                    plain = true;
                }
            }
            let set = set.unwrap_or(true);
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                OPTIMIZED_TREE => self.optimized_tree = set,
                PROP => self.prop = set,
                CLEARED_BY_BUILD => self.cleared_by_build = set,
                WALK_ON_TOP_OF_WALL => self.walk_on_top_of_wall = set,
                _ => {}
            }
        }
        Ok(())
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
            Block::Object if matches!(word, "kindof" | "fencewidth" | "scale" | "isbridge") => {
                let open = self
                    .object
                    .as_mut()
                    .expect("an object block holds an object");
                let object = &mut open.object;
                // A reskin's table sets its fence's width; the game's release build skips a field
                // the table lacks, as its kinds, its scale and whether it is a bridge.
                match word {
                    "fencewidth" => object.fence_width = at.real()?,
                    _ if open.reskin => {}
                    "kindof" => object.kinds.set_by(&tokens[1..], line)?,
                    "isbridge" => object.is_bridge = at.yes()?,
                    _ => {
                        let scale = ModelScale::new(at.real()?);
                        object.scale = scale.ok_or(IniError::Scale { line })?;
                    }
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
    /// reskin, an optimized tree, fluff, a wall a unit walks on, and the fixture map's tank and
    /// its fence.
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
End\r
Object FortressWall\r
  KindOf = WALK_ON_TOP_OF_WALL IMMOBILE\r
End\r
Object Rocks1\r
  KindOf = CLEARED_BY_BUILD\r
  FenceWidth = 10.0\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, OBJECTS};

    /// Objects of each kind a placed object can be, their reskins, and lists of each form.
    const PLACED: &str = "Object Oak
  KindOf = OPTIMIZED_TREE SHRUBBERY
End
Object Bench
  KindOf = prop
  Scale = 1.5
End
ObjectReskin BenchRed Bench
  KindOf = NONE
  Scale = 3.0
End
Object Bush
  KindOf = CLEARED_BY_BUILD SHRUBBERY
End
Object Gravel
  KindOf = CLEARED_BY_BUILD
  FenceWidth = 0.0
End
Object Fence
  KindOf = CLEARED_BY_BUILD
  FenceWidth = 10.0
End
ObjectReskin FenceDown Fence
  FenceWidth = 0.0
End
ObjectReskin FenceRed Fence
End
Object Tank
  KindOf = VEHICLE
End
Object Cleared
  KindOf = PROP
  KindOf = VEHICLE
End
Object Added
  KindOf = VEHICLE
  KindOf = +OPTIMIZED_TREE
End
Object Removed
  KindOf = PROP OPTIMIZED_TREE
  KindOf = -OPTIMIZED_TREE -PROP
End
Object Emptied
  KindOf = PROP
  KindOf = NONE PROP
End
Object Bridge
  IsBridge = Yes
End
ObjectReskin BridgeRed Bridge
  IsBridge = No
End
Object Wall
  KindOf = WALK_ON_TOP_OF_WALL PROP
End
Object Crossing
  IsBridge = no
End
";

    fn draws_of(ini: &ObjectIni, name: &str) -> Vec<DrawModule> {
        ini.objects[name].draws().to_vec()
    }

    #[test]
    fn objects_give_their_default_models_and_looks_with_reskins_copied_as_read() {
        let mut ini = ObjectIni::default();
        ini.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
        ini.read(OBJECTS.as_bytes()).unwrap();
        let names: Vec<&str> = ini.objects().map(|object| object.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "AmericaTankCrusader",
                "Bush",
                "FortressWall",
                "Oak",
                "Rock",
                "RockRed",
                "Rocks1",
                "Stump",
                "Tank",
                "TankCamo",
                "TankRed",
                "Tree"
            ]
        );
        // The tank's default state: its model; its hides and shows, the second of `Barrel`
        // changing the first in its place; its weapon bones, a bone `None` none; `Turret =
        // TurretBone` is a field, and the debris draw module names no model.
        let model = |model: &str, look| DrawModule {
            kind: DrawKind::Family,
            model: Some(model.to_owned()),
            look,
        };
        let change = |name: &str, hide| HideShow {
            name: name.to_owned(),
            hide,
        };
        let tank_look = DefaultLook {
            hide_show: vec![
                change("Turret", true),
                change("Barrel", true),
                change("Hatch", true),
            ],
            weapons: [
                WeaponBones {
                    muzzle_flash: Some("MuzzleFX".to_owned()),
                    ..WeaponBones::default()
                },
                WeaponBones {
                    fire_fx: Some("Fire".to_owned()),
                    ..WeaponBones::default()
                },
                WeaponBones::default(),
            ],
        };
        let tank = [
            model("TANK", tank_look),
            DrawModule {
                kind: DrawKind::Other,
                model: None,
                look: DefaultLook::default(),
            },
        ];
        assert_eq!(draws_of(&ini, "Tank"), tank);
        // A reskin of no draw module keeps its base's; one of its own keeps none of them, and its
        // `ShowSubObject = None` empties the list before `Hatch`.
        assert_eq!(draws_of(&ini, "TankCamo"), tank);
        let red = DefaultLook {
            hide_show: vec![change("Hatch", false)],
            ..DefaultLook::default()
        };
        assert_eq!(draws_of(&ini, "TankRed"), [model("TANK", red)]);
        // The tree names its model by `ModelName`; the stump's `NONE` is no model; a first state
        // of no flag is the default.
        let tree = DrawModule {
            kind: DrawKind::Tree,
            model: Some("PTOak01".to_owned()),
            look: DefaultLook::default(),
        };
        assert_eq!(draws_of(&ini, "Tree"), [tree]);
        let stump = draws_of(&ini, "Stump");
        assert_eq!((stump[0].kind, &stump[0].model), (DrawKind::Prop, &None));
        assert_eq!(
            draws_of(&ini, "Rock"),
            [model("Rock01", DefaultLook::default())]
        );
        // A template finds its object of its case alone, as `findTemplate` does.
        assert_eq!(
            ini.object(b"Tank").map(|tank| tank.name.as_str()),
            Some("Tank")
        );
        assert!(ini.object(b"tank").is_none());
        // A name read before, in this file or an earlier one, and a reskin whose base is not yet
        // read, are refused at their line.
        let mut again = ObjectIni::default();
        again.read(b"Object Rock\nEnd\n").unwrap();
        assert_eq!(
            again.read(b"Object Sand\nEnd\nObjectReskin Rock Sand\nEnd\n"),
            Err(IniError::Redefined { line: 3 })
        );
        assert_eq!(
            again.read(b"ObjectReskin Gravel Pebble\nEnd\nObject Pebble\nEnd\n"),
            Err(IniError::NoBase { line: 1 })
        );
    }

    #[test]
    fn kinds_fences_and_scales_place_objects_as_a_multiplayer_game_does() {
        let mut ini = ObjectIni::default();
        ini.read(PLACED.as_bytes()).unwrap();
        let placed = |name: &str| ini.objects[name].placed_as();
        let scale = |name: &str| ini.objects[name].scale().get();
        // A tree, a prop, and fluff: a build clears it, and its fence is 0 wide, or none; a fence
        // a build clears, and an object of neither kind, are units.
        assert_eq!(placed("Oak"), PlacedAs::Tree);
        assert_eq!(placed("Bench"), PlacedAs::Prop);
        assert_eq!(placed("Bush"), PlacedAs::Prop);
        assert_eq!(placed("Gravel"), PlacedAs::Prop);
        assert_eq!(placed("Fence"), PlacedAs::Unit);
        assert_eq!(placed("Tank"), PlacedAs::Unit);
        // A reskin keeps its base's kinds and scale, whatever its own lines give, and its own
        // fence's width, or its base's: a fence of width 0 is fluff, a fence's reskin a fence.
        assert_eq!(placed("BenchRed"), PlacedAs::Prop);
        assert_eq!(placed("FenceDown"), PlacedAs::Prop);
        assert_eq!(placed("FenceRed"), PlacedAs::Unit);
        assert_eq!((scale("Bench"), scale("BenchRed")), (1.5, 1.5));
        assert_eq!(scale("Oak"), 1.0);
        // A plain list clears the kinds before it sets its own; `+` and `-` keep the others;
        // `NONE` clears them all, and ends the list.
        assert_eq!(placed("Cleared"), PlacedAs::Unit);
        assert_eq!(placed("Added"), PlacedAs::Tree);
        assert_eq!(placed("Removed"), PlacedAs::Unit);
        assert_eq!(placed("Emptied"), PlacedAs::Unit);
        // A landmark bridge and a wall a unit walks on are bridge-like, whatever else they are;
        // a bridge's reskin is one, as its table cannot say otherwise.
        assert_eq!(placed("Bridge"), PlacedAs::Bridge);
        assert_eq!(placed("BridgeRed"), PlacedAs::Bridge);
        assert_eq!(placed("Wall"), PlacedAs::Bridge);
        assert_eq!(placed("Crossing"), PlacedAs::Unit);
        // A list mixes no forms, and `NONE` follows none; a scale is positive.
        let read = |text: &str| ObjectIni::default().read(text.as_bytes());
        for kinds in [
            "PROP +SHRUBBERY",
            "+PROP SHRUBBERY",
            "PROP NONE",
            "+PROP NONE",
        ] {
            let text = format!("Object A\n KindOf = {kinds}\nEnd\n");
            assert_eq!(
                read(&text),
                Err(IniError::MixedKinds { line: 2 }),
                "{kinds}"
            );
        }
        for value in ["0.0", "-1.0", "inf"] {
            let text = format!("Object A\n Scale = {value}\nEnd\n");
            assert_eq!(read(&text), Err(IniError::Scale { line: 2 }), "{value}");
        }
        assert!(matches!(
            read("Object A\n FenceWidth = wide\nEnd\n"),
            Err(IniError::Real { line: 2, .. })
        ));
    }

    #[test]
    fn a_file_the_game_cannot_read_is_refused_at_its_line() {
        let read = |text: &str| ObjectIni::default().read(text.as_bytes());
        assert_eq!(
            read("Object A\n  Draw = W3DModelDraw M\n"),
            Err(IniError::Unclosed)
        );
        assert_eq!(
            read("Object A\nEnd\nEnd\n"),
            Err(IniError::StrayEnd { line: 3 })
        );
        assert_eq!(read("Weapon A\nEnd\n"), Err(IniError::NotBlock { line: 1 }));
        assert_eq!(read("Object\nEnd\n"), Err(IniError::NoName { line: 1 }));
        assert_eq!(
            read("ObjectReskin A\nEnd\n"),
            Err(IniError::NoName { line: 1 })
        );
        let state =
            |line: &str| format!("Object A\n Draw = W3DModelDraw M\n  {line}\n  End\n End\nEnd\n");
        assert_eq!(
            read(&state("ConditionState = DAMAGED")),
            Err(IniError::NoDefaultState { line: 3 })
        );
        // A default state of no model; one after another state.
        assert_eq!(
            read(&state("ConditionState NONE")),
            Err(IniError::NoModel { line: 4 })
        );
        let late = "Object A\n Draw = W3DModelDraw M\n  ConditionState\n   Model = X\n  End\n  DefaultConditionState\n";
        assert_eq!(read(late), Err(IniError::LateDefaultState { line: 6 }));
        let field = |line: &str| {
            format!(
                "Object A\n Draw = W3DModelDraw M\n  DefaultConditionState\n   {line}\n  End\n End\nEnd\n"
            )
        };
        assert_eq!(
            read(&field("WeaponMuzzleFlash = QUATERNARY Flash")),
            Err(IniError::WeaponSlot { line: 4 })
        );
        assert_eq!(
            read(&field("WeaponLaunchBone = PRIMARY")),
            Err(IniError::WeaponSlot { line: 4 })
        );
        // `ArmorSet` alone opens a block: after `;`, which starts a comment, it is alone and needs
        // its `End`; before `//`, which the game does not strip, it is a field.
        assert_eq!(read("Object A\n ArmorSet ; note\n End\nEnd\n"), Ok(()));
        assert_eq!(
            read("Object A\n ArmorSet ; note\nEnd\n"),
            Err(IniError::Unclosed)
        );
        assert_eq!(read("Object A\n ArmorSet // note\nEnd\n"), Ok(()));
    }
}
