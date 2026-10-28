use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{FlagSet, IniLine, IniText};

/// The object creation lists of Zero Hour's `ObjectCreationList.ini`, as
/// `ObjectCreationListStore` reads them: a name read again replaces the list read before.
#[derive(Debug, Default)]
pub(crate) struct CreationListIni {
    lists: BTreeMap<String, Vec<CreationNugget>>,
}

/// A nugget of a list: what it creates, of objects by name or of debris models, or a nugget of
/// another kind, which the import does not read past its kind.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CreationNugget {
    Objects(Creation),
    Debris(Creation),
    Other(CreationKind),
}

/// A kind of nugget past a creation's (`TheObjectCreationListFieldParse`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CreationKind {
    DeliverPayload,
    ApplyRandomForce,
    FireWeapon,
    Attack,
}

/// A `CreateObject` or a `CreateDebris`, each field as `GenericObjectCreationNugget` reads it: its
/// names, objects' or models', `Offset` in world units on the game's axes, `z` up, `Count`, `Mass`
/// and its dispositions with their intensity, its random force's magnitudes and pitches, the
/// pitches in degrees, `SpinRate` in degrees a second, none for the game's −1, and whether it
/// makes its objects inside the object it is made for, `ContainInsideSourceObject`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Creation {
    pub(crate) names: Vec<String>,
    pub(crate) offset: [f32; 3],
    pub(crate) count: i32,
    pub(crate) mass: f32,
    pub(crate) disposition: Disposition,
    pub(crate) intensity: f32,
    pub(crate) force: [f32; 2],
    pub(crate) pitch: [f32; 2],
    pub(crate) spin: Option<f32>,
    pub(crate) inside: bool,
}

/// The dispositions of a creation, `DispositionNames` by their flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Disposition {
    pub(crate) like_existing: bool,
    pub(crate) on_ground_aligned: bool,
    pub(crate) send_it_flying: bool,
    pub(crate) send_it_up: bool,
    pub(crate) send_it_out: bool,
    pub(crate) random_force: bool,
    pub(crate) floating: bool,
    pub(crate) inherit_velocity: bool,
    pub(crate) whirling: bool,
}

/// A nugget being read: a creation's, or one of another kind, which skips its fields.
#[derive(Debug)]
enum Open {
    Objects(Creation),
    Debris(Creation),
    Other(CreationKind),
}

impl CreationListIni {
    /// Reads one INI file of `ObjectCreationList` blocks, each of nuggets, each a block of its own
    /// up to its `End`.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut list: Option<(String, Vec<CreationNugget>)> = None;
        let mut nugget: Option<Open> = None;
        // The blocks open within a nugget, a payload's `DeliveryDecal` the one kind of them.
        let mut inner = 0_u32;
        for line in text.lines().iter() {
            let word = line.word();
            let Some((_, nuggets)) = &mut list else {
                let number = line.number;
                match word {
                    "objectcreationlist" => {
                        let name = line
                            .tokens
                            .get(1)
                            .ok_or(IniError::NoName { line: number })?;
                        list = Some(((*name).to_owned(), Vec::new()));
                    }
                    "end" => return Err(IniError::StrayEnd { line: number }),
                    _ => return Err(IniError::NotBlock { line: number }),
                }
                continue;
            };
            if nugget.is_some() && word == "deliverydecal" && line.tokens.len() == 1 {
                inner += 1;
                continue;
            }
            if word == "end" && inner > 0 {
                inner -= 1;
                continue;
            }
            if word == "end" {
                match nugget.take() {
                    Some(Open::Objects(creation)) => {
                        nuggets.push(CreationNugget::Objects(creation));
                    }
                    Some(Open::Debris(creation)) => nuggets.push(CreationNugget::Debris(creation)),
                    Some(Open::Other(kind)) => nuggets.push(CreationNugget::Other(kind)),
                    None => {
                        let (name, nuggets) = list.take().expect("a list is open");
                        self.lists.insert(name, nuggets);
                    }
                }
                continue;
            }
            match &mut nugget {
                None => nugget = Some(Self::open(word, &line)?),
                Some(Open::Objects(creation) | Open::Debris(creation)) => creation.set(&line)?,
                Some(Open::Other(_)) => {}
            }
        }
        if list.is_some() {
            return Err(IniError::Unclosed);
        }
        Ok(())
    }

    /// Its lists' nuggets, by the lists' names.
    pub(crate) const fn lists(&self) -> &BTreeMap<String, Vec<CreationNugget>> {
        &self.lists
    }

    /// The nugget a line of `word` in a list opens.
    fn open(word: &str, line: &IniLine<'_>) -> Result<Open, IniError> {
        Ok(match word {
            "createobject" => Open::Objects(Creation::new()),
            "createdebris" => Open::Debris(Creation::new()),
            "deliverpayload" => Open::Other(CreationKind::DeliverPayload),
            "applyrandomforce" => Open::Other(CreationKind::ApplyRandomForce),
            "fireweapon" => Open::Other(CreationKind::FireWeapon),
            "attack" => Open::Other(CreationKind::Attack),
            _ => return Err(IniError::NotBlock { line: line.number }),
        })
    }
}

impl Creation {
    /// A creation's fields before its block sets them (`GenericObjectCreationNugget`'s
    /// constructor): one, aligned on the ground, of no spin rate of its own, the rest 0.
    const fn new() -> Self {
        Self {
            names: Vec::new(),
            offset: [0.0; 3],
            count: 1,
            mass: 0.0,
            disposition: Disposition {
                on_ground_aligned: true,
                ..Disposition::NONE
            },
            intensity: 0.0,
            force: [0.0; 2],
            pitch: [0.0; 2],
            spin: None,
            inside: false,
        }
    }

    /// Sets the field `line` names, of those the import reads; the others it skips.
    fn set(&mut self, line: &IniLine<'_>) -> Result<(), IniError> {
        match line.word() {
            "objectnames" | "modelnames" => self
                .names
                .extend(line.tokens[1..].iter().map(|name| (*name).to_owned())),
            "offset" => self.offset = line.coord(["X", "Y", "Z"])?,
            "count" => self.count = line.int()?,
            "mass" => self.mass = line.real()?,
            "disposition" => line.set_flags(&mut self.disposition)?,
            "dispositionintensity" => self.intensity = line.real()?,
            "minforcemagnitude" => self.force[0] = line.real()?,
            "maxforcemagnitude" => self.force[1] = line.real()?,
            "minforcepitch" => self.pitch[0] = line.real()?,
            "maxforcepitch" => self.pitch[1] = line.real()?,
            "spinrate" => self.spin = Some(line.real()?),
            "containinsidesourceobject" => self.inside = line.yes()?,
            _ => {}
        }
        Ok(())
    }
}

impl Disposition {
    const NONE: Self = Self {
        like_existing: false,
        on_ground_aligned: false,
        send_it_flying: false,
        send_it_up: false,
        send_it_out: false,
        random_force: false,
        floating: false,
        inherit_velocity: false,
        whirling: false,
    };
}

impl FlagSet for Disposition {
    fn clear(&mut self) {
        *self = Self::NONE;
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        let flag = match name.to_ascii_lowercase().as_str() {
            "like_existing" => &mut self.like_existing,
            "on_ground_aligned" => &mut self.on_ground_aligned,
            "send_it_flying" => &mut self.send_it_flying,
            "send_it_up" => &mut self.send_it_up,
            "send_it_out" => &mut self.send_it_out,
            "random_force" => &mut self.random_force,
            "floating" => &mut self.floating,
            "inherit_velocity" => &mut self.inherit_velocity,
            "whirling" => &mut self.whirling,
            _ => return false,
        };
        *flag = on;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tank's death of its hulk and a debris piece, a turret's of a random force, and a payload.
    const LISTS: &str = "ObjectCreationList OCL_TankDeath
  CreateObject
    ObjectNames = DeadTankHulk
    Disposition = SEND_IT_FLYING
    DispositionIntensity = 0.1
  End
  CreateDebris
    ModelNames = GXDebris_D01 GXDebris_D02
    Offset = X:17.5 Y:2.0 Z:10.0
    Mass = 5.0
    Count = 2
    Disposition = SEND_IT_FLYING
    DispositionIntensity = 2.5
    BounceSound = VehicleDebris
  End
End
ObjectCreationList OCL_TankTurret
  CreateDebris
    ModelNames        = AVTurret
    Mass              = 50.0
    Disposition       = RANDOM_FORCE INHERIT_VELOCITY
    MinForceMagnitude = 100
    MaxForceMagnitude = 150
    MinForcePitch     = 85
    MaxForcePitch     = 90
    SpinRate          = 300
  End
  DeliverPayload
    Transport = Plane
    DeliveryDecal
      Texture = Scorch
    End
  End
End
";

    #[test]
    fn a_list_reads_its_creations_and_counts_the_rest_by_kind() {
        let mut ini = CreationListIni::default();
        ini.read(LISTS.as_bytes()).unwrap();
        let death = &ini.lists()["OCL_TankDeath"];
        let flying = Disposition {
            send_it_flying: true,
            ..Disposition::NONE
        };
        assert_eq!(
            death[0],
            CreationNugget::Objects(Creation {
                names: vec!["DeadTankHulk".to_owned()],
                disposition: flying,
                intensity: 0.1,
                ..Creation::new()
            })
        );
        assert_eq!(
            death[1],
            CreationNugget::Debris(Creation {
                names: vec!["GXDebris_D01".to_owned(), "GXDebris_D02".to_owned()],
                offset: [17.5, 2.0, 10.0],
                count: 2,
                mass: 5.0,
                disposition: flying,
                intensity: 2.5,
                ..Creation::new()
            })
        );
        let turret = &ini.lists()["OCL_TankTurret"];
        assert_eq!(
            turret[0],
            CreationNugget::Debris(Creation {
                names: vec!["AVTurret".to_owned()],
                mass: 50.0,
                disposition: Disposition {
                    random_force: true,
                    inherit_velocity: true,
                    ..Disposition::NONE
                },
                force: [100.0, 150.0],
                pitch: [85.0, 90.0],
                spin: Some(300.0),
                ..Creation::new()
            })
        );
        assert_eq!(
            turret[1],
            CreationNugget::Other(CreationKind::DeliverPayload)
        );
        // A disposition of no name the game lists, and a nugget of none, are refused.
        let refused = |from: &str, to: &str| {
            CreationListIni::default()
                .read(LISTS.replacen(from, to, 1).as_bytes())
                .unwrap_err()
        };
        assert_eq!(
            refused("SEND_IT_FLYING", "SEND_IT_AWAY"),
            IniError::NotListed { line: 4 }
        );
        assert_eq!(
            refused("  DeliverPayload", "  DeliverPizza"),
            IniError::NotBlock { line: 28 }
        );
    }
}
