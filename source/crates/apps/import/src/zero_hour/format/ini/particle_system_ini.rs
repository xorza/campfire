use std::collections::BTreeMap;

use campfire_client_files::Spread;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniLine, IniText};

/// The particle systems of Zero Hour's `ParticleSystem.ini` files, as `ParticleSystemManager`
/// reads them: a name read again sets its fields over the system read before, as
/// `parseParticleSystemDefinition` finds it and parses into it.
#[derive(Debug, Default)]
pub(crate) struct ParticleSystemIni {
    systems: BTreeMap<String, IniParticleSystem>,
}

/// A particle system's template, each field as the game reads it, in its units: frames, world
/// units and radians on the game's axes, `z` up, alphas and colors from 0 to 1.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniParticleSystem {
    pub(crate) priority: ParticlePriority,
    pub(crate) shader: ParticleShader,
    pub(crate) kind: ParticleKind,
    /// `ParticleName`: its texture's file.
    pub(crate) texture: String,
    pub(crate) angle_z: Spread,
    pub(crate) angular_rate_z: Spread,
    pub(crate) angular_damping: Spread,
    pub(crate) velocity_damping: Spread,
    pub(crate) gravity: f32,
    /// `SlaveSystem`, empty for none, and `SlavePosOffset`.
    pub(crate) slave: String,
    pub(crate) slave_offset: [f32; 3],
    /// `PerParticleAttachedSystem`, empty for none.
    pub(crate) attached: String,
    pub(crate) lifetime: Spread,
    pub(crate) system_lifetime: u32,
    pub(crate) size: Spread,
    pub(crate) start_size_rate: Spread,
    pub(crate) size_rate: Spread,
    pub(crate) size_rate_damping: Spread,
    /// `Alpha1` to `Alpha8`, each its alpha and its frame.
    pub(crate) alpha: [IniKey<Spread>; KEYS],
    /// `Color1` to `Color8`, each its color and its frame.
    pub(crate) color: [IniKey<[f32; 3]>; KEYS],
    /// `ColorScale`: what each channel gains a frame.
    pub(crate) color_scale: Spread,
    pub(crate) burst_delay: Spread,
    pub(crate) burst_count: Spread,
    pub(crate) initial_delay: Spread,
    pub(crate) drift: [f32; 3],
    pub(crate) velocity: VelocityKind,
    pub(crate) ortho: [Spread; 3],
    pub(crate) spherical: Spread,
    pub(crate) hemispherical: Spread,
    pub(crate) cylindrical_radial: Spread,
    pub(crate) cylindrical_normal: Spread,
    pub(crate) outward: Spread,
    pub(crate) outward_other: Spread,
    pub(crate) volume: VolumeKind,
    pub(crate) line_start: [f32; 3],
    pub(crate) line_end: [f32; 3],
    pub(crate) box_half: [f32; 3],
    pub(crate) sphere_radius: f32,
    pub(crate) cylinder_radius: f32,
    pub(crate) cylinder_length: f32,
    pub(crate) hollow: bool,
    pub(crate) ground_aligned: bool,
    pub(crate) above_ground_only: bool,
    pub(crate) up_towards_emitter: bool,
    pub(crate) wind: WindMotion,
}

/// A key of a particle's alpha or color: its value and its frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniKey<T> {
    pub(crate) value: T,
    pub(crate) frame: u32,
}

/// The keys of a particle's alpha and of its color a template holds (`MAX_KEYFRAMES`).
pub(crate) const KEYS: usize = 8;

/// `Priority`, in the game's order, lowest first (`ParticlePriorityNames`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ParticlePriority {
    WeaponExplosion,
    Scorchmark,
    DustTrail,
    Buildup,
    DebrisTrail,
    UnitDamageFx,
    DeathExplosion,
    SemiConstant,
    Constant,
    WeaponTrail,
    AreaEffect,
    Critical,
    AlwaysRender,
}

/// `Shader` (`ParticleShaderTypeNames`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParticleShader {
    None,
    Additive,
    Alpha,
    AlphaTest,
    Multiply,
}

/// `Type` (`ParticleTypeNames`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParticleKind {
    None,
    Particle,
    Drawable,
    Streak,
    VolumeParticle,
    Smudge,
}

/// `VelocityType` (`EmissionVelocityTypeNames`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VelocityKind {
    None,
    Ortho,
    Spherical,
    Hemispherical,
    Cylindrical,
    Outward,
}

/// `VolumeType` (`EmissionVolumeTypeNames`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VolumeKind {
    None,
    Point,
    Line,
    Box,
    Sphere,
    Cylinder,
}

/// `WindMotion` (`WindMotionNames`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindMotion {
    None,
    Unused,
    PingPong,
    Circular,
}

impl ParticleSystemIni {
    /// Reads one INI file, adding its systems to those read before, or setting a system's fields
    /// read before again.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        let text = IniText::of(bytes);
        for block in text.lines().blocks(BlockKinds::One("particlesystem")) {
            let block = block?;
            let system = self
                .systems
                .entry(block.name()?.to_owned())
                .or_insert_with(IniParticleSystem::new);
            for line in &block.fields {
                system.set(line)?;
            }
        }
        Ok(())
    }

    /// Its systems, by their names.
    pub(crate) const fn systems(&self) -> &BTreeMap<String, IniParticleSystem> {
        &self.systems
    }
}

impl IniParticleSystem {
    /// A template's fields before its block sets them (`ParticleSystemInfo`'s constructor); a
    /// field it leaves 0 there, or as its memory pool clears it, is 0, its kind's first, `NONE`.
    const fn new() -> Self {
        let key = IniKey {
            value: Spread::ZERO,
            frame: 0,
        };
        let color = IniKey {
            value: [0.0; 3],
            frame: 0,
        };
        Self {
            priority: ParticlePriority::WeaponExplosion,
            shader: ParticleShader::None,
            kind: ParticleKind::None,
            texture: String::new(),
            angle_z: Spread::ZERO,
            angular_rate_z: Spread::ZERO,
            angular_damping: Spread::ZERO,
            velocity_damping: Spread::ZERO,
            gravity: 0.0,
            slave: String::new(),
            slave_offset: [0.0; 3],
            attached: String::new(),
            lifetime: Spread::ZERO,
            system_lifetime: 0,
            size: Spread::ZERO,
            start_size_rate: Spread::ZERO,
            size_rate: Spread::ZERO,
            size_rate_damping: Spread::ZERO,
            alpha: [key; KEYS],
            color: [color; KEYS],
            color_scale: Spread::ZERO,
            burst_delay: Spread::ZERO,
            burst_count: Spread::ZERO,
            initial_delay: Spread::ZERO,
            drift: [0.0; 3],
            velocity: VelocityKind::None,
            ortho: [Spread::ZERO; 3],
            spherical: Spread::ZERO,
            hemispherical: Spread::ZERO,
            cylindrical_radial: Spread::ZERO,
            cylindrical_normal: Spread::ZERO,
            outward: Spread::ZERO,
            outward_other: Spread::ZERO,
            volume: VolumeKind::None,
            line_start: [0.0; 3],
            line_end: [0.0; 3],
            box_half: [0.0; 3],
            sphere_radius: 0.0,
            cylinder_radius: 0.0,
            cylinder_length: 0.0,
            hollow: false,
            ground_aligned: false,
            above_ground_only: false,
            up_towards_emitter: false,
            wind: WindMotion::None,
        }
    }

    /// Sets the field `line` names, as the template's field table parses it; a field it does not
    /// name, as the wind's angles, the import does not read.
    fn set(&mut self, line: &IniLine<'_>) -> Result<(), IniError> {
        const XYZ: [&str; 3] = ["X", "Y", "Z"];
        let word = line.word();
        if let Some(at) = Self::key_at(word, "alpha") {
            self.alpha[at] = IniKey {
                value: Spread {
                    low: line.real_at(1)?,
                    high: line.real_at(2)?,
                },
                frame: Self::frame(line, 3)?,
            };
            return Ok(());
        }
        if let Some(at) = Self::key_at(word, "color") {
            let rgb = line.int_coord(["R", "G", "B"])?;
            let value = rgb.map(|channel| {
                u8::try_from(channel)
                    .map(|channel| f32::from(channel) / 255.0)
                    .map_err(|_| IniError::Range { line: line.number })
            });
            let [r, g, b] = value;
            self.color[at] = IniKey {
                value: [r?, g?, b?],
                frame: Self::frame(line, line.tokens.len() - 1)?,
            };
            return Ok(());
        }
        match word {
            "priority" => self.priority = Self::listed(line, &PRIORITIES)?,
            "shader" => self.shader = Self::listed(line, &SHADERS)?,
            "type" => self.kind = Self::listed(line, &KINDS)?,
            "particlename" => self.texture = Self::text(line),
            "anglez" => self.angle_z = line.random()?,
            "angularratez" => self.angular_rate_z = line.random()?,
            "angulardamping" => self.angular_damping = line.random()?,
            "velocitydamping" => self.velocity_damping = line.random()?,
            "gravity" => self.gravity = line.real()?,
            "slavesystem" => self.slave = Self::text(line),
            "slaveposoffset" => self.slave_offset = line.coord(XYZ)?,
            "perparticleattachedsystem" => self.attached = Self::text(line),
            "lifetime" => self.lifetime = line.random()?,
            "systemlifetime" => self.system_lifetime = Self::frame(line, 1)?,
            "size" => self.size = line.random()?,
            "startsizerate" => self.start_size_rate = line.random()?,
            "sizerate" => self.size_rate = line.random()?,
            "sizeratedamping" => self.size_rate_damping = line.random()?,
            "colorscale" => {
                // In 255ths of a channel, as a color key's, which the system divides as it starts
                // (`ParticleSystem::ParticleSystem`).
                let scale = line.random()?;
                self.color_scale = Spread {
                    low: scale.low / 255.0,
                    high: scale.high / 255.0,
                };
            }
            "burstdelay" => self.burst_delay = line.random()?,
            "burstcount" => self.burst_count = line.random()?,
            "initialdelay" => self.initial_delay = line.random()?,
            "driftvelocity" => self.drift = line.coord(XYZ)?,
            "velocitytype" => self.velocity = Self::listed(line, &VELOCITIES)?,
            "velorthox" => self.ortho[0] = line.random()?,
            "velorthoy" => self.ortho[1] = line.random()?,
            "velorthoz" => self.ortho[2] = line.random()?,
            "velspherical" => self.spherical = line.random()?,
            "velhemispherical" => self.hemispherical = line.random()?,
            "velcylindricalradial" => self.cylindrical_radial = line.random()?,
            "velcylindricalnormal" => self.cylindrical_normal = line.random()?,
            "veloutward" => self.outward = line.random()?,
            "veloutwardother" => self.outward_other = line.random()?,
            "volumetype" => self.volume = Self::listed(line, &VOLUMES)?,
            "vollinestart" => self.line_start = line.coord(XYZ)?,
            "vollineend" => self.line_end = line.coord(XYZ)?,
            "volboxhalfsize" => self.box_half = line.coord(XYZ)?,
            "volsphereradius" => self.sphere_radius = line.real()?,
            "volcylinderradius" => self.cylinder_radius = line.real()?,
            "volcylinderlength" => self.cylinder_length = line.real()?,
            "ishollow" => self.hollow = line.yes()?,
            "isgroundaligned" => self.ground_aligned = Self::listed(line, &ALIGNED)?,
            "isemitabovegroundonly" => self.above_ground_only = line.yes()?,
            "isparticleuptowardsemitter" => self.up_towards_emitter = line.yes()?,
            "windmotion" => self.wind = Self::listed(line, &WINDS)?,
            _ => {}
        }
        Ok(())
    }

    /// The place of the key `word` names among those of `field`, `<field>1` to `<field>8`.
    fn key_at(word: &str, field: &str) -> Option<usize> {
        let number: usize = word.strip_prefix(field)?.parse().ok()?;
        (1..=KEYS).contains(&number).then(|| number - 1)
    }

    /// The whole number of no sign of `line`'s token at `at`, as `scanUnsignedInt` reads it.
    fn frame(line: &IniLine<'_>, at: usize) -> Result<u32, IniError> {
        u32::try_from(line.int_at(at)?).map_err(|_| IniError::Range { line: line.number })
    }

    /// The value of `line`'s field of `names`, as `parseIndexList` matches it, of either case.
    fn listed<T: Copy>(line: &IniLine<'_>, names: &[(&str, T)]) -> Result<T, IniError> {
        let token = line.value()?;
        let found = names
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(token));
        found
            .map(|&(_, value)| value)
            .ok_or(IniError::NotListed { line: line.number })
    }

    /// The text of `line`'s field, empty for none, as `parseAsciiString` reads it.
    fn text(line: &IniLine<'_>) -> String {
        line.tokens.get(1).copied().unwrap_or_default().to_owned()
    }
}

const PRIORITIES: [(&str, ParticlePriority); 13] = [
    ("WEAPON_EXPLOSION", ParticlePriority::WeaponExplosion),
    ("SCORCHMARK", ParticlePriority::Scorchmark),
    ("DUST_TRAIL", ParticlePriority::DustTrail),
    ("BUILDUP", ParticlePriority::Buildup),
    ("DEBRIS_TRAIL", ParticlePriority::DebrisTrail),
    ("UNIT_DAMAGE_FX", ParticlePriority::UnitDamageFx),
    ("DEATH_EXPLOSION", ParticlePriority::DeathExplosion),
    ("SEMI_CONSTANT", ParticlePriority::SemiConstant),
    ("CONSTANT", ParticlePriority::Constant),
    ("WEAPON_TRAIL", ParticlePriority::WeaponTrail),
    ("AREA_EFFECT", ParticlePriority::AreaEffect),
    ("CRITICAL", ParticlePriority::Critical),
    ("ALWAYS_RENDER", ParticlePriority::AlwaysRender),
];
const SHADERS: [(&str, ParticleShader); 5] = [
    ("NONE", ParticleShader::None),
    ("ADDITIVE", ParticleShader::Additive),
    ("ALPHA", ParticleShader::Alpha),
    ("ALPHA_TEST", ParticleShader::AlphaTest),
    ("MULTIPLY", ParticleShader::Multiply),
];
const KINDS: [(&str, ParticleKind); 6] = [
    ("NONE", ParticleKind::None),
    ("PARTICLE", ParticleKind::Particle),
    ("DRAWABLE", ParticleKind::Drawable),
    ("STREAK", ParticleKind::Streak),
    ("VOLUME_PARTICLE", ParticleKind::VolumeParticle),
    ("SMUDGE", ParticleKind::Smudge),
];
const VELOCITIES: [(&str, VelocityKind); 6] = [
    ("NONE", VelocityKind::None),
    ("ORTHO", VelocityKind::Ortho),
    ("SPHERICAL", VelocityKind::Spherical),
    ("HEMISPHERICAL", VelocityKind::Hemispherical),
    ("CYLINDRICAL", VelocityKind::Cylindrical),
    ("OUTWARD", VelocityKind::Outward),
];
const VOLUMES: [(&str, VolumeKind); 6] = [
    ("NONE", VolumeKind::None),
    ("POINT", VolumeKind::Point),
    ("LINE", VolumeKind::Line),
    ("BOX", VolumeKind::Box),
    ("SPHERE", VolumeKind::Sphere),
    ("CYLINDER", VolumeKind::Cylinder),
];
const ALIGNED: [(&str, bool); 2] = [("No", false), ("Yes", true)];
const WINDS: [(&str, WindMotion); 4] = [
    ("NONE", WindMotion::None),
    ("Unused", WindMotion::Unused),
    ("PingPong", WindMotion::PingPong),
    ("Circular", WindMotion::Circular),
];
#[cfg(test)]
pub(crate) mod samples {
    /// A smoke trail of a cylinder's velocity and a hollow cylinder, and a spark of a sphere
    /// that names the smoke its slave, then the spark again, which sets its size over its own.
    pub(crate) const PARTICLE_SYSTEMS: &str = "ParticleSystem SmokeTrail
  Priority = WEAPON_TRAIL
  IsOneShot = No
  Shader = ALPHA
  Type = PARTICLE
  ParticleName = EXSmoke.tga
  AngleZ = 0.00 6.00
  AngularRateZ = -0.01 0.01
  AngularDamping = 1.00 1.00
  VelocityDamping = 0.95 0.95
  Gravity = -0.01
  SlaveSystem =
  SlavePosOffset = X:4.00 Y:5.00 Z:6.00
  PerParticleAttachedSystem =
  Lifetime = 30.00 45.00
  SystemLifetime = 0
  Size = 1.00 2.00
  StartSizeRate = 0.00 0.00
  SizeRate = 0.50 0.50
  SizeRateDamping = 0.98 0.98
  Alpha1 = 0.00 0.00 0
  Alpha2 = 0.50 0.75 5
  Alpha3 = 0.00 0.00 30
  Alpha4 = 0.00 0.00 0
  Color1 = R:255 G:128 B:0 0
  Color2 = R:0 G:0 B:51 20
  ColorScale = -5.10 2.55
  BurstDelay = 2.00 4.00
  BurstCount = 1.00 1.00
  InitialDelay = 0.00 0.00 GAUSSIAN
  DriftVelocity = X:0.00 Y:0.00 Z:0.10
  VelocityType = CYLINDRICAL
  VelCylindricalRadial = 0.10 0.20
  VelCylindricalNormal = 1.00 1.50
  VolumeType = CYLINDER
  VolCylinderRadius = 3.00
  VolCylinderLength = 2.00
  IsHollow = Yes
  IsGroundAligned = No
  IsEmitAboveGroundOnly = Yes
  IsParticleUpTowardsEmitter = No
  WindMotion = Unused
End
ParticleSystem Spark
  Priority = ALWAYS_RENDER
  IsOneShot = Yes
  Shader = ADDITIVE
  Type = PARTICLE
  ParticleName = EXSpark.tga
  SlaveSystem = SmokeTrail
  SlavePosOffset = X:1.00 Y:2.00 Z:3.00
  Lifetime = 10.00 10.00
  Size = 1.00 1.00
  Alpha1 = 1.00 1.00 0
  Color1 = R:255 G:255 B:255 0
  BurstCount = 8.00 8.00 CONSTANT
  VelocityType = SPHERICAL
  VelSpherical = 1.00 2.00
  VolumeType = SPHERE
  VolSphereRadius = 4.00
End
ParticleSystem Spark
  Size = 3.00 3.00
End
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::particle_system_ini::samples::PARTICLE_SYSTEMS;

    #[test]
    fn a_system_takes_each_field_and_a_block_of_its_name_again_sets_its_fields_over_it() {
        let mut ini = ParticleSystemIni::default();
        ini.read(PARTICLE_SYSTEMS.as_bytes()).unwrap();
        assert_eq!(ini.systems().len(), 2);
        let smoke = &ini.systems()["SmokeTrail"];
        assert_eq!(smoke.priority, ParticlePriority::WeaponTrail);
        assert_eq!(
            (smoke.shader, smoke.kind, smoke.texture.as_str()),
            (ParticleShader::Alpha, ParticleKind::Particle, "EXSmoke.tga")
        );
        assert_eq!(
            smoke.angle_z,
            Spread {
                low: 0.0,
                high: 6.0
            }
        );
        assert_eq!((smoke.slave.as_str(), smoke.attached.as_str()), ("", ""));
        assert_eq!(
            smoke.alpha[1].value,
            Spread {
                low: 0.5,
                high: 0.75
            }
        );
        assert_eq!((smoke.alpha[2].frame, smoke.alpha[3].frame), (30, 0));
        // A color's channels over 255, its frame its last token: 128 / 255 and 51 / 255.
        assert_eq!(smoke.color[0].value, [1.0, 128.0 / 255.0, 0.0]);
        assert_eq!(
            smoke.color[1],
            IniKey {
                value: [0.0, 0.0, 0.2],
                frame: 20
            }
        );
        // A color scale in 255ths of a channel a frame: −5.1 / 255 and 2.55 / 255.
        assert_eq!(
            smoke.color_scale,
            Spread {
                low: -5.1 / 255.0,
                high: 2.55 / 255.0
            }
        );
        // A distribution the game draws as 0.
        assert_eq!(smoke.initial_delay, Spread::ZERO);
        assert_eq!(
            smoke.burst_delay,
            Spread {
                low: 2.0,
                high: 4.0
            }
        );
        assert_eq!(smoke.drift, [0.0, 0.0, 0.1]);
        assert_eq!(smoke.velocity, VelocityKind::Cylindrical);
        assert_eq!(
            (smoke.cylindrical_radial, smoke.cylindrical_normal),
            (
                Spread {
                    low: 0.1,
                    high: 0.2
                },
                Spread {
                    low: 1.0,
                    high: 1.5
                }
            )
        );
        assert_eq!(
            (smoke.volume, smoke.cylinder_radius, smoke.cylinder_length),
            (VolumeKind::Cylinder, 3.0, 2.0)
        );
        assert!(smoke.hollow && smoke.above_ground_only && !smoke.ground_aligned);
        assert_eq!(smoke.wind, WindMotion::Unused);

        // The spark's second block sets its size and keeps the rest.
        let spark = &ini.systems()["Spark"];
        assert_eq!(spark.size, Spread::of(3.0));
        // `IsOneShot`, which no code of the game reads, is not read.
        assert_eq!(
            (spark.priority, spark.shader),
            (ParticlePriority::AlwaysRender, ParticleShader::Additive)
        );
        assert_eq!(
            (spark.slave.as_str(), spark.slave_offset),
            ("SmokeTrail", [1.0, 2.0, 3.0])
        );
        assert_eq!(spark.burst_count, Spread::of(8.0));
        assert_eq!(
            (spark.volume, spark.sphere_radius),
            (VolumeKind::Sphere, 4.0)
        );
        // A field the block does not set keeps the template's default.
        assert_eq!(
            (spark.angular_damping, spark.velocity),
            (Spread::ZERO, VelocityKind::Spherical)
        );

        let fails = |text: &[u8]| ParticleSystemIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"ParticleSystem A\n  Shader = GLOW\nEnd\n"),
            IniError::NotListed { line: 2 }
        );
        assert_eq!(
            fails(b"ParticleSystem A\n  Color1 = R:256 G:0 B:0 0\nEnd\n"),
            IniError::Range { line: 2 }
        );
        assert_eq!(
            fails(b"ParticleSystem A\n  Lifetime = 1 2 SKEWED\nEnd\n"),
            IniError::NotListed { line: 2 }
        );
        assert_eq!(fails(b"FXList A\nEnd\n"), IniError::NotBlock { line: 1 });
    }
}
