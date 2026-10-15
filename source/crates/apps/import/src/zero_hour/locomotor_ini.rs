use std::collections::BTreeMap;

use crate::zero_hour::error::IniError;
use crate::zero_hour::ini_text::{FlagList, IniLine, IniText};

/// The locomotors of Zero Hour's `Locomotor.ini` files, as `LocomotorStore` reads them, by their
/// names, of their case, as `NAMEKEY` finds them.
#[derive(Debug, Default)]
pub(crate) struct LocomotorIni {
    locomotors: BTreeMap<String, Locomotor>,
}

/// What the import reads of a locomotor (`LocomotorTemplate`): the surfaces it moves on, its
/// `Speed` in units a second and its `TurnRate` in degrees a second, each 0 when absent, as the
/// template starts.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Locomotor {
    surfaces: Surfaces,
    pub(crate) speed: f32,
    pub(crate) turn_rate: f32,
}

/// The surfaces of a locomotor's `Surfaces`, a bit each in the order of
/// `TheLocomotorSurfaceTypeNames`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Surfaces(u8);

/// `TheLocomotorSurfaceTypeNames`, in order.
const SURFACES: [&str; 5] = ["ground", "water", "cliff", "air", "rubble"];

impl LocomotorIni {
    /// Reads one INI file, adding its locomotors to those read before: one read again keeps its
    /// fields and takes the new ones, as a template found again is read into.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut open: Option<String> = None;
        for line in text.lines() {
            let word = line.word();
            match &open {
                None if word == "locomotor" => {
                    let name = *line
                        .tokens
                        .get(1)
                        .ok_or(IniError::NoName { line: line.number })?;
                    self.locomotors.entry(name.to_owned()).or_default();
                    open = Some(name.to_owned());
                }
                None if word == "end" => return Err(IniError::StrayEnd { line: line.number }),
                None => return Err(IniError::NotBlock { line: line.number }),
                Some(_) if word == "end" => open = None,
                Some(name) => self
                    .locomotors
                    .get_mut(name)
                    .expect("an open locomotor is in the list")
                    .field(&word, &line)?,
            }
        }
        match open {
            None => Ok(()),
            Some(_) => Err(IniError::Unclosed),
        }
    }

    /// The locomotor `name`; none for a name no file defines.
    pub(crate) fn get(&self, name: &str) -> Option<&Locomotor> {
        self.locomotors.get(name)
    }
}

impl Locomotor {
    /// Whether it moves on the ground.
    pub(crate) const fn drives(&self) -> bool {
        self.surfaces.0 & 1 != 0
    }

    /// Takes the line of `word`, a field the import reads or another.
    fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        match word {
            "surfaces" => self.surfaces.set_by(at)?,
            "speed" => self.speed = at.real()?,
            "turnrate" => self.turn_rate = at.real()?,
            _ => {}
        }
        Ok(())
    }
}

impl Surfaces {
    /// Sets the surfaces by a `Surfaces` line's flags, as `INI::parseBitString32` does; an error
    /// for a name no surface has.
    fn set_by(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        let bit = |name: &str| {
            SURFACES
                .iter()
                .position(|surface| name.eq_ignore_ascii_case(surface))
                .map(|at| 1 << at)
                .ok_or(IniError::NotListed { line: at.number })
        };
        match at.flags()? {
            FlagList::Clear => self.0 = 0,
            FlagList::Only(names) => {
                self.0 = 0;
                for name in names {
                    self.0 |= bit(name)?;
                }
            }
            FlagList::Change(changes) => {
                for change in changes {
                    let bit = bit(change.name)?;
                    if change.set {
                        self.0 |= bit;
                    } else {
                        self.0 &= !bit;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// `Data\INI\Locomotor.ini` of the fixture install: a tank's treads, which drive on the
    /// ground and over rubble, and a jet's wings, which only fly.
    pub(crate) const LOCOMOTORS: &str = "Locomotor TankTreads\r
  Surfaces = GROUND RUBBLE\r
  Speed = 30.5 ; units a second\r
  TurnRate = 90\r
  Acceleration = 30\r
End\r
Locomotor JetWings\r
  Surfaces = AIR\r
  Speed = 120\r
  TurnRate = 45\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::locomotor_ini::internals::LOCOMOTORS;

    #[test]
    fn a_locomotor_takes_its_surfaces_speed_and_turn_rate_a_later_file_last() {
        let mut ini = LocomotorIni::default();
        ini.read(LOCOMOTORS.as_bytes()).unwrap();
        let treads = *ini.get("TankTreads").unwrap();
        assert_eq!((treads.speed, treads.turn_rate), (30.5, 90.0));
        // Ground and rubble, bits 0 and 4: it drives; the jet's air alone, bit 3, does not.
        assert_eq!(treads.surfaces, Surfaces(0b1_0001));
        assert!(treads.drives());
        assert!(!ini.get("JetWings").unwrap().drives());
        // A later file keeps the fields it does not name, and `+` and `-` change the surfaces it
        // names alone; names are of their case, as `NAMEKEY` keeps them.
        ini.read(b"locomotor TankTreads\n surfaces = -GROUND +AIR\n speed = 15\nEnd\n")
            .unwrap();
        let slowed = *ini.get("TankTreads").unwrap();
        assert_eq!((slowed.speed, slowed.turn_rate), (15.0, 90.0));
        assert_eq!(slowed.surfaces, Surfaces(0b1_1000));
        assert!(!slowed.drives());
        assert!(ini.get("tanktreads").is_none());
        // A new locomotor of no field holds none of them; `NONE` clears its surfaces.
        ini.read(b"Locomotor Still\nEnd\nLocomotor Grounded\n Surfaces = GROUND\n Surfaces = NONE\nEnd\n")
            .unwrap();
        assert_eq!(ini.get("Still"), Some(&Locomotor::default()));
        assert_eq!(ini.get("Grounded").unwrap().surfaces, Surfaces(0));

        let fails = |text: &[u8]| LocomotorIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"Locomotor A\n Surfaces = GROUND MUD\nEnd\n"),
            IniError::NotListed { line: 2 }
        );
        assert_eq!(
            fails(b"Locomotor A\n Surfaces = GROUND +AIR\nEnd\n"),
            IniError::MixedFlags { line: 2 }
        );
        assert!(matches!(
            fails(b"Locomotor A\n Speed = fast\nEnd\n"),
            IniError::Real { line: 2, .. }
        ));
        assert_eq!(fails(b"Speed = 1\n"), IniError::NotBlock { line: 1 });
        assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
        assert_eq!(fails(b"Locomotor\nEnd\n"), IniError::NoName { line: 1 });
        assert_eq!(fails(b"Locomotor A\n"), IniError::Unclosed);
    }
}
