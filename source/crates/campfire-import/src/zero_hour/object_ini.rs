use std::collections::BTreeMap;

use crate::zero_hour::error::IniError;

/// The objects of Zero Hour's object INI files, as `ThingFactory` reads them, each with what its
/// draw modules show by default.
#[derive(Debug, Default)]
pub(crate) struct ObjectIni {
    /// Each object by its name in lowercase, a later definition replacing an earlier one.
    objects: BTreeMap<String, IniObject>,
}

/// An object: its name as the INI writes it, the object it reskins, and its own draw modules.
#[derive(Debug)]
pub(crate) struct IniObject {
    pub(crate) name: String,
    base: Option<String>,
    draws: Vec<DrawModule>,
}

/// A draw module: the model of its default state, if it draws one, and that state's look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DrawModule {
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

/// The draw modules whose `ModelName` names the model.
const NAMED_DRAWS: [&str; 2] = ["w3dtreedraw", "w3dpropdraw"];

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
enum DrawKind {
    /// The `Model` of its default state, as the `W3DModelDraw` family does.
    Family,
    /// Its `ModelName`.
    Named,
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
        // The game reads bytes; Latin-1 keeps each as one char.
        let text: String = bytes.iter().map(|&byte| char::from(byte)).collect();
        let mut reading = Reading::default();
        for (number, raw) in text.split('\n').enumerate() {
            let line = number + 1;
            let content: String = raw
                .split(';')
                .next()
                .unwrap_or_default()
                .chars()
                .map(|c| if c < ' ' { ' ' } else { c })
                .collect();
            let tokens: Vec<&str> = content
                .split([' ', '='])
                .filter(|token| !token.is_empty())
                .collect();
            let Some(&first) = tokens.first() else {
                continue;
            };
            let word = first.to_ascii_lowercase();
            if word == "end" {
                if let Some(done) = reading.end(line)? {
                    self.objects.insert(done.name.to_ascii_lowercase(), done);
                }
            } else {
                reading.line(&word, &tokens, line)?;
            }
        }
        if reading.stack.is_empty() {
            Ok(())
        } else {
            Err(IniError::Unclosed)
        }
    }

    /// Every object, by its name in lowercase.
    pub(crate) fn objects(&self) -> impl Iterator<Item = &IniObject> {
        self.objects.values()
    }

    /// The draw modules `object` has: its own, or, for a reskin that declares none, its base's,
    /// as the game drops every copied draw module at an object's first own one.
    pub(crate) fn draws<'a>(&'a self, object: &'a IniObject) -> &'a [DrawModule] {
        let mut at = object;
        // A chain of reskins ends at an object or at a name the files lack; a cycle, which the
        // game cannot build, ends after as many steps as there are objects.
        for _ in 0..=self.objects.len() {
            match (&at.base, at.draws.is_empty()) {
                (Some(base), true) => match self.objects.get(base) {
                    Some(next) => at = next,
                    None => return &[],
                },
                _ => return &at.draws,
            }
        }
        &[]
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
    fn line(&mut self, word: &str, tokens: &[&str], line: usize) -> Result<(), IniError> {
        match self.stack.last_mut() {
            None => {
                let base = match word {
                    "object" => None,
                    "objectreskin" => Some(tokens.get(2).ok_or(IniError::NoName { line })?),
                    _ => return Err(IniError::NotObject { line }),
                };
                let name = tokens.get(1).ok_or(IniError::NoName { line })?;
                self.object = Some(IniObject {
                    name: (*name).to_owned(),
                    base: base.map(|base| base.to_ascii_lowercase()),
                    draws: Vec::new(),
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
                    "modelname" if *kind == DrawKind::Named => {
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
                let kind = if MODEL_DRAWS.contains(&module.as_str()) {
                    DrawKind::Family
                } else if NAMED_DRAWS.contains(&module.as_str()) {
                    DrawKind::Named
                } else {
                    DrawKind::Other
                };
                self.draw = Some(DrawModule {
                    model: None,
                    look: DefaultLook::default(),
                });
                self.stack.push(Block::Draw { kind, states: 0 });
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
  Draw = W3DModelDraw ModuleTag_01
    DefaultConditionState
      Model = Absent
    End
  End
End
";

    /// `Data\INI\Object.ini` of the fixture install: two reskins of the tank, one with its own
    /// draw module, a later tree of no model, and an object of a lone mesh.
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
  Draw = W3DModelDraw ModuleTag_01\r
    ConditionState\r
      Model = Rock01\r
    End\r
  End\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, OBJECTS};

    fn draws_of(ini: &ObjectIni, name: &str) -> Vec<DrawModule> {
        ini.draws(&ini.objects[name]).to_vec()
    }

    #[test]
    fn objects_give_their_default_models_and_looks_with_reskins_and_later_definitions() {
        let mut ini = ObjectIni::default();
        ini.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
        ini.read(OBJECTS.as_bytes()).unwrap();
        let names: Vec<&str> = ini.objects().map(|object| object.name.as_str()).collect();
        assert_eq!(names, ["Rock", "Tank", "TankCamo", "TankRed", "Tree"]);
        // The tank's default state: its model; its hides and shows, the second of `Barrel`
        // changing the first in its place; its weapon bones, a bone `None` none; `Turret =
        // TurretBone` is a field, and the debris draw module names no model.
        let model = |model: &str, look| DrawModule {
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
                model: None,
                look: DefaultLook::default(),
            },
        ];
        assert_eq!(draws_of(&ini, "tank"), tank);
        // A reskin of no draw module keeps its base's; one of its own keeps none of them, and its
        // `ShowSubObject = None` empties the list before `Hatch`.
        assert_eq!(draws_of(&ini, "tankcamo"), tank);
        let red = DefaultLook {
            hide_show: vec![change("Hatch", false)],
            ..DefaultLook::default()
        };
        assert_eq!(draws_of(&ini, "tankred"), [model("TANK", red)]);
        // The later tree replaces the earlier, and its `NONE` is no model; a first state of no
        // flag is the default.
        assert_eq!(draws_of(&ini, "tree")[0].model, None);
        assert_eq!(
            draws_of(&ini, "rock"),
            [model("Rock01", DefaultLook::default())]
        );
        // A reskin of a base the files lack has no draw module.
        let mut lost = ObjectIni::default();
        lost.read(b"ObjectReskin Lost Nowhere\nEnd\n").unwrap();
        assert!(draws_of(&lost, "lost").is_empty());
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
        assert_eq!(
            read("Weapon A\nEnd\n"),
            Err(IniError::NotObject { line: 1 })
        );
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
