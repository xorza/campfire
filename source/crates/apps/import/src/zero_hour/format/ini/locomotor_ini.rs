use std::collections::BTreeMap;

use crate::zero_hour::format::game_frames;
use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, FlagSet, IniLine, IniText};

/// The locomotors of Zero Hour's `Locomotor.ini` files, as `LocomotorStore` reads them, by their
/// names, of their case, as `NAMEKEY` finds them.
#[derive(Debug, Default)]
pub(crate) struct LocomotorIni {
    locomotors: BTreeMap<String, Locomotor>,
}

/// What the import reads of a locomotor (`LocomotorTemplate`): the surfaces it moves on, its
/// `Speed` and `MinSpeed` in units a second, its `TurnRate` in degrees a second, its
/// `Acceleration` and its `Lift` in units a second each second, its `CirclingRadius` and its
/// `PreferredHeight` in units, and its `TurnPivotOffset`, a share of its radius, each 0 when
/// absent, as the template starts; its `Appearance`; its `AirborneTargetingHeight`, none for the
/// template's `INT_MAX`; its `SpeedDamaged`, `TurnRateDamaged` and `AccelerationDamaged`, none for
/// its own; its `Braking` in units a second each second and its `MinTurnSpeed` in units a second,
/// none for the template's `BIGNUM`, 99,999 a frame; its `CanMoveBackwards`, false when absent;
/// and its `CloseEnoughDist` in units, 1 when absent.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Locomotor {
    surfaces: Surfaces,
    pub(crate) speed: f32,
    pub(crate) turn_rate: f32,
    pub(crate) acceleration: f32,
    pub(crate) lift: f32,
    pub(crate) min_speed: f32,
    pub(crate) circling_radius: f32,
    pub(crate) preferred_height: f32,
    pub(crate) appearance: Appearance,
    pub(crate) airborne_height: Option<i32>,
    speed_damaged: Option<f32>,
    turn_rate_damaged: Option<f32>,
    acceleration_damaged: Option<f32>,
    braking: Option<f32>,
    min_turn_speed: Option<f32>,
    pub(crate) pivot: f32,
    pub(crate) backwards: bool,
    close_enough: Option<f32>,
    pub(crate) chassis: IniChassis,
}

/// A locomotor's `Appearance`, of those the import tells apart: a missile's `THRUST`, a
/// helicopter's `HOVER`, a jet's `WINGS`, a tank's `TREADS`, a car's `FOUR_WHEELS`, a soldier's
/// `TWO_LEGS`, a bike's `MOTORCYCLE`, or another.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Appearance {
    #[default]
    Other,
    Thrust,
    Hover,
    Wings,
    Treads,
    Wheels,
    Legs,
    Motorcycle,
}

/// The springs a locomotor's chassis pitches and rolls on, as `LocomotorTemplate` reads them: its
/// `PitchStiffness` and `RollStiffness`, 0.1 by default, its `PitchDamping` and `RollDamping`, 0.9
/// by default, its `UniformAxialDamping`, 1 by default, its `ForwardAccelerationPitchFactor` and
/// `LateralAccelerationRollFactor`, 0 by default, and its `AccelerationPitchLimit` and
/// `DecelerationPitchLimit` in degrees, 0 by default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniChassis {
    pub(crate) pitch_stiffness: f32,
    pub(crate) roll_stiffness: f32,
    pub(crate) pitch_damping: f32,
    pub(crate) roll_damping: f32,
    pub(crate) axial_damping: f32,
    pub(crate) forward_pitch: f32,
    pub(crate) lateral_roll: f32,
    pub(crate) accelerating: f32,
    pub(crate) braking: f32,
}

impl Default for IniChassis {
    fn default() -> Self {
        Self {
            pitch_stiffness: 0.1,
            roll_stiffness: 0.1,
            pitch_damping: 0.9,
            roll_damping: 0.9,
            axial_damping: 1.0,
            forward_pitch: 0.0,
            lateral_roll: 0.0,
            accelerating: 0.0,
            braking: 0.0,
        }
    }
}

/// The surfaces of a locomotor's `Surfaces`, a bit each in the order of
/// `TheLocomotorSurfaceTypeNames`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Surfaces(u8);

/// `TheLocomotorSurfaceTypeNames`, in order.
const SURFACES: [&str; 5] = ["ground", "water", "cliff", "air", "rubble"];

/// The bit of `AIR` among the surfaces.
const AIR: usize = 3;

/// The template's number of a braking or a turning speed it leaves out, `BIGNUM`, a frame.
const BIGNUM: f32 = 99_999.0;

impl LocomotorIni {
    /// Reads one INI file, adding its locomotors to those read before: one read again keeps its
    /// fields and takes the new ones, as a template found again is read into.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("locomotor"))
        {
            let block = block?;
            let locomotor = self.locomotors.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                locomotor.field(line.word(), line)?;
            }
        }
        Ok(())
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

    /// Whether it hovers in the air, as a helicopter's does: it moves on `AIR` and appears as
    /// `HOVER`.
    pub(crate) fn hovers(&self) -> bool {
        self.surfaces.0 & (1 << AIR) != 0 && self.appearance == Appearance::Hover
    }

    /// Whether it flies in the air on wings, as a jet's does: it moves on `AIR` and appears as
    /// `WINGS`.
    pub(crate) fn wings(&self) -> bool {
        self.surfaces.0 & (1 << AIR) != 0 && self.appearance == Appearance::Wings
    }

    /// Its acceleration while damaged past the game's `MovementPenaltyDamageState`, as a dead
    /// unit's body always is, its `Acceleration` when it names none, as
    /// `LocomotorTemplate::validate` takes it.
    pub(crate) fn acceleration_damaged(&self) -> f32 {
        self.acceleration_damaged.unwrap_or(self.acceleration)
    }

    /// Its chassis's springs, of a tank's `TREADS` or a car's `FOUR_WHEELS`, whose drawing pitches
    /// and rolls on them; none of another appearance.
    pub(crate) fn chassis(&self) -> Option<&IniChassis> {
        matches!(self.appearance, Appearance::Treads | Appearance::Wheels).then_some(&self.chassis)
    }

    /// Its speed while damaged past the game's `MovementPenaltyDamageState`, its `Speed` when it
    /// names none, as `LocomotorTemplate::validate` takes it.
    pub(crate) fn speed_damaged(&self) -> f32 {
        self.speed_damaged.unwrap_or(self.speed)
    }

    /// Its turn rate while damaged, as `speed_damaged` takes it.
    pub(crate) fn turn_rate_damaged(&self) -> f32 {
        self.turn_rate_damaged.unwrap_or(self.turn_rate)
    }

    /// Its braking in units a second each second, the template's `BIGNUM` a frame each frame when
    /// it names none, as the template holds it in its own frames.
    pub(crate) fn braking(&self) -> f32 {
        self.braking
            .unwrap_or(BIGNUM * game_frames::PER_SECOND * game_frames::PER_SECOND)
    }

    /// The least speed it turns at in units a second, the template's `BIGNUM` a frame when it
    /// names none.
    pub(crate) fn min_turn_speed(&self) -> f32 {
        self.min_turn_speed
            .unwrap_or(BIGNUM * game_frames::PER_SECOND)
    }

    /// The units from its goal within which it has arrived, 1 when it names none.
    pub(crate) fn close_enough(&self) -> f32 {
        self.close_enough.unwrap_or(1.0)
    }

    /// Takes the line of `word`, a field the import reads or another.
    fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        match word {
            "surfaces" => self.surfaces.set_by(at)?,
            "speed" => self.speed = at.real()?,
            "turnrate" => self.turn_rate = at.real()?,
            "acceleration" => self.acceleration = at.real()?,
            "speeddamaged" => self.speed_damaged = Some(at.real()?),
            "turnratedamaged" => self.turn_rate_damaged = Some(at.real()?),
            "accelerationdamaged" => self.acceleration_damaged = Some(at.real()?),
            "braking" => self.braking = Some(at.real()?),
            "minturnspeed" => self.min_turn_speed = Some(at.real()?),
            "turnpivotoffset" => self.pivot = at.real()?,
            "canmovebackwards" => self.backwards = at.yes()?,
            "closeenoughdist" => self.close_enough = Some(at.real()?),
            "pitchstiffness" => self.chassis.pitch_stiffness = at.real()?,
            "rollstiffness" => self.chassis.roll_stiffness = at.real()?,
            "pitchdamping" => self.chassis.pitch_damping = at.real()?,
            "rolldamping" => self.chassis.roll_damping = at.real()?,
            "uniformaxialdamping" => self.chassis.axial_damping = at.real()?,
            "forwardaccelerationpitchfactor" => self.chassis.forward_pitch = at.real()?,
            "lateralaccelerationrollfactor" => self.chassis.lateral_roll = at.real()?,
            "accelerationpitchlimit" => self.chassis.accelerating = at.real()?,
            "decelerationpitchlimit" => self.chassis.braking = at.real()?,
            "lift" => self.lift = at.real()?,
            "minspeed" => self.min_speed = at.real()?,
            "circlingradius" => self.circling_radius = at.real()?,
            "preferredheight" => self.preferred_height = at.real()?,
            "airbornetargetingheight" => self.airborne_height = Some(at.int()?),
            "appearance" => {
                let value = at.value()?;
                self.appearance = if value.eq_ignore_ascii_case("thrust") {
                    Appearance::Thrust
                } else if value.eq_ignore_ascii_case("hover") {
                    Appearance::Hover
                } else if value.eq_ignore_ascii_case("wings") {
                    Appearance::Wings
                } else if value.eq_ignore_ascii_case("treads") {
                    Appearance::Treads
                } else if value.eq_ignore_ascii_case("four_wheels") {
                    Appearance::Wheels
                } else if value.eq_ignore_ascii_case("two_legs") {
                    Appearance::Legs
                } else if value.eq_ignore_ascii_case("motorcycle") {
                    Appearance::Motorcycle
                } else {
                    Appearance::Other
                };
            }
            _ => {}
        }
        Ok(())
    }
}

impl Surfaces {
    /// Sets the surfaces by a `Surfaces` line's flags, as `INI::parseBitString32` does; an error
    /// for a name no surface has.
    fn set_by(&mut self, at: &IniLine<'_>) -> Result<(), IniError> {
        at.set_flags(self)
    }
}

impl FlagSet for Surfaces {
    fn clear(&mut self) {
        self.0 = 0;
    }

    fn set(&mut self, name: &str, on: bool) -> bool {
        let Some(at) = SURFACES
            .iter()
            .position(|surface| name.eq_ignore_ascii_case(surface))
        else {
            return false;
        };
        if on {
            self.0 |= 1 << at;
        } else {
            self.0 &= !(1 << at);
        }
        true
    }
}

#[cfg(test)]
pub(crate) mod samples {
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
Locomotor RaptorWings\r
  Surfaces = AIR\r
  Speed = 175\r
  MinSpeed = 60\r
  Acceleration = 120\r
  TurnRate = 120\r
  Lift = 120\r
  CirclingRadius = 100\r
  PreferredHeight = 100\r
  AirborneTargetingHeight = 30\r
  Appearance = WINGS\r
End\r
Locomotor JetTaxi\r
  Surfaces = GROUND\r
  Speed = 50\r
  TurnRate = 180\r
  Appearance = TREADS\r
End\r
Locomotor HeliRotors\r
  Surfaces = AIR\r
  Speed = 150\r
  TurnRate = 180\r
  Lift = 120\r
  PreferredHeight = 100\r
  AirborneTargetingHeight = 30\r
  Appearance = HOVER\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;

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
        // The helicopter's rotors fly on the air alone, and hover; the jet's wings do not hover,
        // and the treads drive, of no airborne height, the template's `INT_MAX`.
        let rotors = *ini.get("HeliRotors").unwrap();
        assert!(rotors.hovers() && !rotors.drives());
        assert_eq!(
            (rotors.lift, rotors.preferred_height, rotors.airborne_height),
            (120.0, 100.0, Some(30))
        );
        assert!(!ini.get("JetWings").unwrap().hovers() && !treads.hovers());
        // The Raptor's wings fly on the air alone, of a least speed and a circling radius; the
        // jet's plain wings, of no `WINGS` appearance, and the rotors fly on none.
        let raptor = *ini.get("RaptorWings").unwrap();
        assert!(raptor.wings() && !raptor.hovers() && !raptor.drives());
        assert_eq!((raptor.min_speed, raptor.circling_radius), (60.0, 100.0));
        assert!(!ini.get("JetWings").unwrap().wings() && !rotors.wings());
        assert_eq!(treads.airborne_height, None);
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
