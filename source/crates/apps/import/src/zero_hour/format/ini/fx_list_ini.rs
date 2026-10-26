use std::collections::BTreeMap;

use campfire_client_files::Spread;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{IniLine, IniText};

/// The effect lists of Zero Hour's `FXList.ini` files, as `FXListStore` reads them: a name read
/// again replaces the list read before, as `parseFXListDefinition` clears it.
#[derive(Debug, Default)]
pub(crate) struct FxListIni {
    lists: BTreeMap<String, Vec<FxNugget>>,
}

/// A nugget of a list: a particle system it starts, or a nugget of another kind, which the
/// import does not read past its kind.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FxNugget {
    ParticleSystem(ParticleNugget),
    Other(NuggetKind),
}

/// A kind of nugget past a particle system's (`TheFXListFieldParse`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NuggetKind {
    Sound,
    RayEffect,
    Tracer,
    LightPulse,
    ViewShake,
    TerrainScorch,
    FxListAtBonePos,
}

/// A particle system nugget, each field as `ParticleSystemFXNugget::parse` reads it: `Offset` in
/// world units on the game's axes, `z` up; `Radius`, `Height` and `InitialDelay`, in
/// milliseconds, none for the game's −1, random variables; the rotations in radians.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParticleNugget {
    pub(crate) name: String,
    pub(crate) count: i32,
    pub(crate) offset: [f32; 3],
    pub(crate) radius: Spread,
    pub(crate) height: Spread,
    pub(crate) delay: Spread,
    pub(crate) rotate: [f32; 3],
    pub(crate) orient_to_object: bool,
    pub(crate) ricochet: bool,
    pub(crate) attach_to_object: bool,
    pub(crate) at_ground: bool,
    pub(crate) callers_radius: bool,
}

/// A nugget being read: a particle system's, or one of another kind, which skips its fields.
#[derive(Debug)]
enum Open {
    Particles(ParticleNugget),
    Other(NuggetKind),
}

impl FxListIni {
    /// Reads one INI file of `FXList` blocks, each of nuggets, each a block of its own up to its
    /// `End`.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        let mut list: Option<(String, Vec<FxNugget>)> = None;
        let mut nugget: Option<Open> = None;
        for line in text.lines().iter() {
            let word = line.word();
            let Some((_, nuggets)) = &mut list else {
                let number = line.number;
                match word {
                    "fxlist" => {
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
            if word == "end" {
                match nugget.take() {
                    Some(Open::Particles(particles)) => {
                        nuggets.push(FxNugget::ParticleSystem(particles));
                    }
                    Some(Open::Other(kind)) => nuggets.push(FxNugget::Other(kind)),
                    None => {
                        let (name, nuggets) = list.take().expect("a list is open");
                        self.lists.insert(name, nuggets);
                    }
                }
                continue;
            }
            match &mut nugget {
                None => nugget = Some(Self::open(word, &line)?),
                Some(Open::Particles(particles)) => particles.set(&line)?,
                Some(Open::Other(_)) => {}
            }
        }
        if list.is_some() {
            return Err(IniError::Unclosed);
        }
        Ok(())
    }

    /// Its lists' nuggets, by the lists' names.
    pub(crate) const fn lists(&self) -> &BTreeMap<String, Vec<FxNugget>> {
        &self.lists
    }

    /// The nugget a line of `word` in a list opens.
    fn open(word: &str, line: &IniLine<'_>) -> Result<Open, IniError> {
        Ok(match word {
            "particlesystem" => Open::Particles(ParticleNugget::new()),
            "sound" => Open::Other(NuggetKind::Sound),
            "rayeffect" => Open::Other(NuggetKind::RayEffect),
            "tracer" => Open::Other(NuggetKind::Tracer),
            "lightpulse" => Open::Other(NuggetKind::LightPulse),
            "viewshake" => Open::Other(NuggetKind::ViewShake),
            "terrainscorch" => Open::Other(NuggetKind::TerrainScorch),
            "fxlistatbonepos" => Open::Other(NuggetKind::FxListAtBonePos),
            _ => return Err(IniError::NotBlock { line: line.number }),
        })
    }
}

impl ParticleNugget {
    /// A nugget's fields before its block sets them (its constructor): a count of 1, no delay,
    /// the rest 0.
    const fn new() -> Self {
        Self {
            name: String::new(),
            count: 1,
            offset: [0.0; 3],
            radius: Spread::ZERO,
            height: Spread::ZERO,
            delay: Spread::of(-1.0),
            rotate: [0.0; 3],
            orient_to_object: false,
            ricochet: false,
            attach_to_object: false,
            at_ground: false,
            callers_radius: false,
        }
    }

    /// Sets the field `line` names; one it does not name is an error, as the game's table
    /// refuses it.
    fn set(&mut self, line: &IniLine<'_>) -> Result<(), IniError> {
        match line.word() {
            "name" => line.value()?.clone_into(&mut self.name),
            "count" => self.count = line.int()?,
            "offset" => self.offset = line.coord(["X", "Y", "Z"])?,
            "radius" => self.radius = line.random()?,
            "height" => self.height = line.random()?,
            "initialdelay" => self.delay = line.random()?,
            "rotatex" => self.rotate[0] = line.real()?.to_radians(),
            "rotatey" => self.rotate[1] = line.real()?.to_radians(),
            "rotatez" => self.rotate[2] = line.real()?.to_radians(),
            "orienttoobject" => self.orient_to_object = line.yes()?,
            "ricochet" => self.ricochet = line.yes()?,
            "attachtoobject" => self.attach_to_object = line.yes()?,
            "createatgroundheight" => self.at_ground = line.yes()?,
            "usecallersradius" => self.callers_radius = line.yes()?,
            _ => return Err(IniError::NotListed { line: line.number }),
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// A tank's hit, of a smoke trail twice, delayed, and a sound; a list of a sound alone; and
    /// the hit again, which replaces the first.
    pub(crate) const FX_LISTS: &str = "FXList FX_TankHit
  ParticleSystem
    Name = SmokeTrail
    Count = 3
    Offset = X:1.0 Y:2.0 Z:3.0
    Radius = 0 5
    Height = 1 2
    InitialDelay = 100 200
    RotateZ = 90
    OrientToObject = Yes
    CreateAtGroundHeight = Yes
  End
  Sound
    Name = ExplosionSmall
  End
End
FXList FX_Quiet
  Sound
    Name = Hush
  End
End
FXList FX_TankHit
  ParticleSystem
    Name = Spark
  End
  ViewShake
    Type = SUBTLE
  End
  ParticleSystem
    Name = SmokeTrail
    Count = 2
    Radius = 0 3
    InitialDelay = 100 200
  End
End
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::fx_list_ini::samples::FX_LISTS;

    #[test]
    fn a_list_takes_its_nuggets_and_a_list_of_its_name_again_replaces_it() {
        let mut ini = FxListIni::default();
        ini.read(FX_LISTS.as_bytes()).unwrap();
        assert_eq!(ini.lists().len(), 2);
        assert_eq!(
            ini.lists()["FX_Quiet"],
            [FxNugget::Other(NuggetKind::Sound)]
        );
        let hit = &ini.lists()["FX_TankHit"];
        assert_eq!(hit.len(), 3);
        assert_eq!(hit[1], FxNugget::Other(NuggetKind::ViewShake));
        let FxNugget::ParticleSystem(spark) = &hit[0] else {
            panic!("the first is a particle system's");
        };
        // A nugget's defaults: once, now, of no delay.
        assert_eq!(
            (spark.name.as_str(), spark.count, spark.delay),
            ("Spark", 1, Spread::of(-1.0))
        );
        let FxNugget::ParticleSystem(smoke) = &hit[2] else {
            panic!("the last is a particle system's");
        };
        assert_eq!(
            (smoke.count, smoke.radius),
            (
                2,
                Spread {
                    low: 0.0,
                    high: 3.0
                }
            )
        );

        // The first definition's fields, read before the second replaced it.
        let mut first = FxListIni::default();
        let end = FX_LISTS.find("FXList FX_Quiet").unwrap();
        first.read(&FX_LISTS.as_bytes()[..end]).unwrap();
        let FxNugget::ParticleSystem(smoke) = &first.lists()["FX_TankHit"][0] else {
            panic!("the first is a particle system's");
        };
        assert_eq!(smoke.offset, [1.0, 2.0, 3.0]);
        assert_eq!(
            (smoke.height, smoke.delay),
            (
                Spread {
                    low: 1.0,
                    high: 2.0
                },
                Spread {
                    low: 100.0,
                    high: 200.0
                }
            )
        );
        assert_eq!(smoke.rotate, [0.0, 0.0, 90.0_f32.to_radians()]);
        assert!(smoke.orient_to_object && smoke.at_ground && !smoke.attach_to_object);

        let fails = |text: &[u8]| FxListIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"FXList A\n  Glow\n  End\nEnd\n"),
            IniError::NotBlock { line: 2 }
        );
        assert_eq!(
            fails(b"FXList A\n  ParticleSystem\n    Colour = 1\n  End\nEnd\n"),
            IniError::NotListed { line: 3 }
        );
        assert_eq!(fails(b"FXList A\n  Sound\n  End\n"), IniError::Unclosed);
        assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
    }
}
