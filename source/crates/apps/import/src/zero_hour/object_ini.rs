use std::collections::BTreeMap;

use campfire_package::ModelScale;

use crate::zero_hour::error::IniError;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::ini_text::{IniLine, IniText};

/// The objects of Zero Hour's object INI files, as `ThingFactory` reads them, each with what its
/// draw modules show by default.
#[derive(Debug, Default)]
pub(crate) struct ObjectIni {
    /// Each object by its name, of its case, as `ThingFactory::findTemplate` finds it, a later
    /// definition replacing an earlier one.
    objects: BTreeMap<String, IniObject>,
}

/// An object: its name as the INI writes it, the object it reskins, its own draw modules, and
/// the fields of its own that say how the game draws it: its kinds, its fence's width and its
/// asset scale, none of them given when absent.
#[derive(Debug)]
pub(crate) struct IniObject {
    pub(crate) name: String,
    base: Option<String>,
    draws: Vec<DrawModule>,
    kinds: ObjectKinds,
    fence_width: Option<f32>,
    scale: Option<ModelScale>,
}

/// The kinds of an object's `KindOf` that decide who draws its placed objects
/// (`GameLogic::startNewGame`): an optimized tree and a prop the client alone draws, and an
/// object a build clears, which is fluff, the client's too, unless it is a fence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ObjectKinds {
    optimized_tree: bool,
    prop: bool,
    cleared_by_build: bool,
}

/// Who draws an object a map places, as a multiplayer game spawns it (design 12, D14): the
/// client's tree buffer, its prop buffer, or the sim, as a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacedAs {
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
    object: Option<IniObject>,
    draw: Option<DrawModule>,
}

impl ObjectIni {
    /// Reads one INI file, adding its objects to those read before.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut reading = Reading::default();
        for line in text.lines() {
            let word = line.word();
            if word == "end" {
                if let Some(done) = reading.end(line.number)? {
                    self.objects.insert(done.name.clone(), done);
                }
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

    /// The draw modules `object` has: its own, or, for a reskin that declares none, its base's,
    /// as the game drops every copied draw module at an object's first own one.
    pub(crate) fn draws<'a>(&'a self, object: &'a IniObject) -> &'a [DrawModule] {
        self.chain(object)
            .find(|at| at.base.is_none() || !at.draws.is_empty())
            .map_or(&[], |at| &at.draws)
    }

    /// Who draws an object of `object` a map places: an optimized tree the tree buffer; a prop,
    /// or fluff, an object a build clears whose fence is 0 wide, the prop buffer, as a
    /// multiplayer game forces fluff onto it; any other the sim. A reskin keeps its base's kinds,
    /// which its table cannot set, and gives its own fence's width or keeps its base's.
    pub(crate) fn placed_as(&self, object: &IniObject) -> PlacedAs {
        let kinds = self
            .chain(object)
            .find(|at| at.base.is_none())
            .map(|at| at.kinds);
        let kinds = kinds.unwrap_or_default();
        let fence_width = self.chain(object).find_map(|at| at.fence_width);
        if kinds.optimized_tree {
            PlacedAs::Tree
        } else if kinds.prop || (kinds.cleared_by_build && fence_width.unwrap_or(0.0) == 0.0) {
            PlacedAs::Prop
        } else {
            PlacedAs::Unit
        }
    }

    /// The asset scale every drawable of `object` takes: its `Scale`, which a reskin cannot set,
    /// 1 when it gives none.
    pub(crate) fn scale(&self, object: &IniObject) -> ModelScale {
        let own = self.chain(object).find(|at| at.base.is_none());
        own.and_then(|at| at.scale).unwrap_or_default()
    }

    /// `object`, then the object it reskins, and so on: a chain ends at an object, or at a name
    /// the files lack; a cycle, which the game cannot build, ends after as many steps as there are
    /// objects.
    fn chain<'a>(&'a self, object: &'a IniObject) -> impl Iterator<Item = &'a IniObject> {
        let mut next = Some(object);
        std::iter::from_fn(move || {
            let at = next?;
            next = at.base.as_ref().and_then(|base| self.objects.get(base));
            Some(at)
        })
        .take(self.objects.len() + 1)
    }

    /// Sets `kinds` by a `KindOf` line's flags, as `BitFlags::parse` does: `NONE` clears them;
    /// a list of names sets those it names and clears the rest; `+` and `-` set and clear one,
    /// and keep the rest; a list may not mix the two forms.
    fn kind_of(kinds: &mut ObjectKinds, flags: &[&str], line: usize) -> Result<(), IniError> {
        let mut plain = false;
        let mut signed = false;
        for flag in flags {
            if flag.eq_ignore_ascii_case("none") {
                if plain || signed {
                    return Err(IniError::MixedKinds { line });
                }
                *kinds = ObjectKinds::default();
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
                        *kinds = ObjectKinds::default();
                    }
                    plain = true;
                }
            }
            let set = set.unwrap_or(true);
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                OPTIMIZED_TREE => kinds.optimized_tree = set,
                PROP => kinds.prop = set,
                CLEARED_BY_BUILD => kinds.cleared_by_build = set,
                _ => {}
            }
        }
        Ok(())
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

    /// Hides or shows `names`, as `parseShowHideSubObject` does: `NONE` first empties the list,
    /// and a name the list holds keeps its place and takes the new change.
    fn hide_show(list: &mut Vec<HideShow>, names: &[&str], hide: bool) {
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

    /// A field of a default state that the look reads.
    fn default_field(
        module: &mut DrawModule,
        word: &str,
        tokens: &[&str],
        line: usize,
    ) -> Result<(), IniError> {
        match word {
            "model" => module.model = ObjectIni::model(tokens),
            "hidesubobject" | "showsubobject" => {
                ObjectIni::hide_show(
                    &mut module.look.hide_show,
                    &tokens[1..],
                    word == "hidesubobject",
                );
            }
            "weaponfirefxbone" | "weaponrecoilbone" | "weaponmuzzleflash" | "weaponlaunchbone" => {
                let (Some(slot), Some(bone)) = (tokens.get(1), tokens.get(2)) else {
                    return Err(IniError::WeaponSlot { line });
                };
                let slot = SLOTS
                    .iter()
                    .position(|known| slot.eq_ignore_ascii_case(known))
                    .ok_or(IniError::WeaponSlot { line })?;
                let bones = &mut module.look.weapons[slot];
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

impl IniObject {
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
    /// Closes the innermost block; the object it closes, if it is one.
    fn end(&mut self, line: usize) -> Result<Option<IniObject>, IniError> {
        match self.stack.pop().ok_or(IniError::StrayEnd { line })? {
            Block::Object => Ok(Some(
                self.object.take().expect("an object block holds an object"),
            )),
            Block::Draw { .. } => {
                let done = self.draw.take().expect("a draw block holds a module");
                self.object
                    .as_mut()
                    .expect("a draw is in an object")
                    .draws
                    .push(done);
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
        match self.stack.last_mut() {
            None => {
                let base = match word {
                    "object" => None,
                    "objectreskin" => Some(tokens.get(2).ok_or(IniError::NoName { line })?),
                    _ => return Err(IniError::NotBlock { line }),
                };
                let name = tokens.get(1).ok_or(IniError::NoName { line })?;
                self.object = Some(IniObject {
                    name: (*name).to_owned(),
                    base: base.map(|base| (*base).to_owned()),
                    draws: Vec::new(),
                    kinds: ObjectKinds::default(),
                    fence_width: None,
                    scale: None,
                });
                self.stack.push(Block::Object);
            }
            Some(Block::Draw { kind, states }) => {
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
            Some(Block::DefaultState { named }) => {
                let module = self.draw.as_mut().expect("a state is in a draw");
                *named |= word == "model";
                ObjectIni::default_field(module, word, tokens, line)?;
                if ObjectIni::opens(word, tokens) {
                    self.stack.push(Block::Other);
                }
            }
            Some(Block::Object) if word == "draw" => {
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
            Some(Block::Object) if matches!(word, "kindof" | "fencewidth" | "scale") => {
                let object = self
                    .object
                    .as_mut()
                    .expect("an object block holds an object");
                // A reskin's table sets its fence's width; the game's release build skips a field
                // the table lacks, as its kinds and its scale.
                match word {
                    "fencewidth" => object.fence_width = Some(at.real()?),
                    _ if object.base.is_some() => {}
                    "kindof" => ObjectIni::kind_of(&mut object.kinds, &tokens[1..], line)?,
                    _ => {
                        let scale = ModelScale::new(at.real()?);
                        object.scale = Some(scale.ok_or(IniError::Scale { line })?);
                    }
                }
            }
            Some(_) if ObjectIni::opens(word, tokens) => self.stack.push(Block::Other),
            Some(_) => {}
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
    /// draw module, a later tree of no model, a prop of a lone mesh at a scale of 1.5 and its
    /// reskin, an optimized tree, fluff, and the fixture map's tank and its fence.
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
Object Tree\r
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
";

    fn draws_of(ini: &ObjectIni, name: &str) -> Vec<DrawModule> {
        ini.draws(&ini.objects[name]).to_vec()
    }

    #[test]
    fn objects_give_their_default_models_and_looks_with_reskins_and_later_definitions() {
        let mut ini = ObjectIni::default();
        ini.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
        ini.read(OBJECTS.as_bytes()).unwrap();
        let names: Vec<&str> = ini.objects().map(|object| object.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "AmericaTankCrusader",
                "Bush",
                "Oak",
                "Rock",
                "RockRed",
                "Rocks1",
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
        // The later tree replaces the earlier, and its `NONE` is no model; a first state of no
        // flag is the default.
        let tree = draws_of(&ini, "Tree");
        assert_eq!((tree[0].kind, &tree[0].model), (DrawKind::Prop, &None));
        assert_eq!(
            draws_of(&ini, "Rock"),
            [model("Rock01", DefaultLook::default())]
        );
        // A reskin of a base the files lack has no draw module.
        let mut lost = ObjectIni::default();
        lost.read(b"ObjectReskin Lost Nowhere\nEnd\n").unwrap();
        assert!(draws_of(&lost, "Lost").is_empty());
        // A template finds its object of its case alone, as `findTemplate` does.
        assert_eq!(
            ini.object(b"Tank").map(|tank| tank.name.as_str()),
            Some("Tank")
        );
        assert!(ini.object(b"tank").is_none());
        let tree = DrawModule {
            kind: DrawKind::Tree,
            model: Some("PTOak01".to_owned()),
            look: DefaultLook::default(),
        };
        let mut first = ObjectIni::default();
        first.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
        assert_eq!(draws_of(&first, "Tree"), [tree]);
    }

    #[test]
    fn kinds_fences_and_scales_place_objects_as_a_multiplayer_game_does() {
        let mut ini = ObjectIni::default();
        ini.read(PLACED.as_bytes()).unwrap();
        let placed = |name: &str| ini.placed_as(&ini.objects[name]);
        let scale = |name: &str| ini.scale(&ini.objects[name]).get();
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
