use std::collections::{BTreeMap, BTreeSet};

use bevy_math::{Quat, Vec3};
use campfire_client_files::{
    ActionLook, AlphaKey, Blend, ClientUnit, ColorKey, DeathLook, EffectList, EffectsFile,
    EmissionVelocity, EmissionVolume, ListedSystem, ParticleSystem, SlaveSystem, Spread,
};
use campfire_common::DeclaredName;
use campfire_values::PackagePath;

use crate::zero_hour::archive_path::ArchivePath;
use crate::zero_hour::axes;
use crate::zero_hour::dropped::{Dropped, Losses};
use crate::zero_hour::effect_drop_cause::EffectDropCause;
use crate::zero_hour::error::{NameClash, NameError, ZeroHourError};
use crate::zero_hour::format::ini::fx_list_ini::{FxListIni, FxNugget, NuggetKind, ParticleNugget};
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::particle_system_ini::{
    IniKey, IniParticleSystem, KEYS, ParticleKind, ParticlePriority, ParticleShader,
    ParticleSystemIni, VelocityKind, VolumeKind, WindMotion,
};
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
use crate::zero_hour::format::w3d::w3d_mesh::CUTOFF;
use crate::zero_hour::game_path;
use crate::zero_hour::import_name::ImportName;

/// The install's particle systems and effect lists as the engine's effects file
/// (effects.md, Effects are a format of the engine; effects.md, The import hands the client
/// consistent effects): each list of the systems its nuggets start, and each system a list or
/// another system starts, those of what the slice leaves dropped and counted, each name resolved
/// to a system it holds; and the textures its systems draw, each once, for the import to convert.
#[derive(Debug)]
pub(crate) struct EffectsImport {
    pub(crate) file: EffectsFile,
    pub(crate) textures: BTreeSet<ArchivePath>,
    pub(crate) dropped: Vec<Dropped<EffectDropCause>>,
}

/// The sources of the effects: the install's systems and lists, the systems its weapons'
/// projectiles carry, by their names, the most particles alive at once, and where the archives
/// hold a texture of a name.
#[derive(Debug)]
pub(crate) struct EffectSources<'a, F> {
    pub(crate) systems: &'a ParticleSystemIni,
    pub(crate) lists: &'a FxListIni,
    pub(crate) trails: &'a [&'a str],
    pub(crate) cap: u32,
    pub(crate) texture: F,
}

/// How the import converts a weapon's action: not at all; as a shot of no projectile; or as one
/// that fires projectiles of the unit type it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WeaponShot<'a> {
    Dropped,
    Shot,
    Projectile(&'a DeclaredName),
}

/// The game's logic frame, in seconds, the step its systems count in (`LOGICFRAMES_PER_SECOND`).
const STEP: f32 = 1.0 / 30.0;
/// A millisecond in steps, as the game converts a nugget's delay
/// (`ConvertDurationFromMsecsToFrames`).
const STEPS_PER_MS: f32 = 30.0 / 1000.0;

impl EffectsImport {
    /// The effects of `sources`; an error for two systems or two lists whose names give one, a
    /// texture of a name no package path holds, or a number not finite.
    pub(crate) fn of<F: Fn(&str) -> Option<ArchivePath>>(
        sources: EffectSources<'_, F>,
    ) -> Result<Self, ZeroHourError> {
        let mut losses = Losses::default();
        let names = Self::names(sources.systems.systems().keys())?;
        let mut drawn = BTreeMap::new();
        for (name, system) in sources.systems.systems() {
            let cause = match (system.kind, system.shader) {
                (ParticleKind::Particle, ParticleShader::None) => Some(EffectDropCause::Shader),
                (ParticleKind::Particle, _) => None,
                _ => Some(EffectDropCause::Kind),
            };
            let texture = (sources.texture)(&system.texture);
            match (cause, texture) {
                (Some(cause), _) => losses.drop(name, cause),
                (None, None) => losses.drop(name, EffectDropCause::Texture),
                (None, Some(texture)) => {
                    drawn.insert(name.as_str(), texture);
                }
            }
        }
        let mut lists = BTreeMap::new();
        let mut open = Vec::new();
        let list_names = Self::names(sources.lists.lists().keys())?;
        for (name, nuggets) in sources.lists.lists() {
            let mut systems = Vec::new();
            for nugget in nuggets {
                let particles = match nugget {
                    FxNugget::ParticleSystem(particles) => particles,
                    FxNugget::Other(kind) => {
                        losses.drop(name, Self::nugget_cause(*kind));
                        continue;
                    }
                };
                if !drawn.contains_key(particles.name.as_str()) {
                    if !sources.systems.systems().contains_key(&particles.name) {
                        losses.drop(name, EffectDropCause::NoSystem);
                    }
                    continue;
                }
                if let Some(entry) = Self::entry(particles, &names) {
                    open.push(particles.name.as_str());
                    systems.push(entry);
                }
            }
            if !systems.is_empty() {
                lists.insert(list_names[name.as_str()].clone(), EffectList { systems });
            }
        }
        open.extend(
            sources
                .trails
                .iter()
                .filter(|trail| drawn.contains_key(*trail)),
        );
        let mut kept = BTreeSet::new();
        while let Some(name) = open.pop() {
            if !kept.insert(name) {
                continue;
            }
            let system = &sources.systems.systems()[name];
            open.extend(
                [system.slave.as_str(), system.attached.as_str()]
                    .into_iter()
                    .filter(|named| drawn.contains_key(named)),
            );
        }
        let mut systems = BTreeMap::new();
        let mut textures = BTreeSet::new();
        for name in kept {
            let system = &sources.systems.systems()[name];
            let texture = drawn[name].clone();
            // A slave's particles stand off by the slave's own `SlavePosOffset`, as
            // `mergeRelatedParticleSystems` reads the slave's.
            let slave_offset = sources
                .systems
                .systems()
                .get(&system.slave)
                .map_or([0.0; 3], |slave| slave.slave_offset);
            let texture_path = game_path::texture(&texture)?;
            let converted = Self::system(system, &texture_path, slave_offset, |named| {
                if named.is_empty() {
                    return None;
                }
                if !drawn.contains_key(named) {
                    if !sources.systems.systems().contains_key(named) {
                        losses.drop(name, EffectDropCause::NoSystem);
                    }
                    return None;
                }
                Some(names[named].clone())
            });
            if !matches!(system.wind, WindMotion::None | WindMotion::Unused) {
                losses.drop(name, EffectDropCause::Wind);
            }
            textures.insert(texture);
            systems.insert(names[name].clone(), converted);
        }
        // The import resolves every name and orders every key, so the file refuses a number of
        // the install's alone, one not finite.
        let file = EffectsFile::new(STEP, sources.cap, systems, lists)?;
        Ok(Self {
            file,
            textures,
            dropped: losses.into_iter().collect(),
        })
    }

    /// Names, in `looks`, the effects each unit type of an object of `objects` plays as it dies,
    /// of the lists it holds: at its start the `DeathFX` of its first `FXListDie` that applies and
    /// the first FX list of the `INITIAL` phase of the slow death a normal death most likely
    /// takes, and at its middle and its end that death's `MIDPOINT` and `FINAL` (effects.md, A
    /// death plays its phases). It counts what it leaves of each: a phase's other lists, the
    /// objects and the weapons of its phases, and its draws' tread dust and track marks. An error
    /// for an object of no name a package holds.
    pub(crate) fn name_deaths(
        &mut self,
        objects: &ObjectIni,
        looks: &mut BTreeMap<DeclaredName, ClientUnit>,
    ) -> Result<(), NameError> {
        let mut losses = Losses::default();
        for object in objects.objects() {
            let Some(look) = looks.get_mut(&ImportName::object(object).declared()?) else {
                continue;
            };
            let phases = object.slow_death().map(|death| &death.phases);
            let firsts = phases.map(|phases| {
                phases
                    .each_ref()
                    .map(|phase| phase.effects.first().map(String::as_str))
            });
            let [initial, midpoint, last] = firsts.unwrap_or_default();
            look.death = DeathLook {
                start: self.lists(object.death_effect().into_iter().chain(initial))?,
                middle: self.lists(midpoint)?,
                end: self.lists(last)?,
            };
            let name = &object.name;
            for phase in phases.into_iter().flatten() {
                if phase.effects.len() > 1 {
                    losses.drop(name, EffectDropCause::DeathChoice);
                }
                if phase.objects > 0 {
                    losses.drop(name, EffectDropCause::DeathObject);
                }
                if phase.weapons > 0 {
                    losses.drop(name, EffectDropCause::DeathWeapon);
                }
            }
            for draw in object.draws() {
                if draw.tread_dust {
                    losses.drop(name, EffectDropCause::TreadDust);
                }
                if draw.track_marks {
                    losses.drop(name, EffectDropCause::TrackMarks);
                }
            }
        }
        self.dropped.extend(losses);
        Ok(())
    }

    /// What each action of a weapon of `weapons` plays, as `delivery` gives each by its id:
    /// its fire's list, and, of a shot of no projectile, its hit's; and, in `units`, what the
    /// type of its projectile plays, its hit's list where it ends and its exhaust as it flies;
    /// each of those the file holds. None for an action that plays nothing.
    pub(crate) fn action_looks<'d>(
        &self,
        weapons: &WeaponIni,
        delivery: impl Fn(&DeclaredName) -> WeaponShot<'d>,
        units: &mut BTreeMap<DeclaredName, ClientUnit>,
    ) -> Result<BTreeMap<DeclaredName, ActionLook>, NameError> {
        let mut looks = BTreeMap::new();
        for (name, weapon) in weapons.weapons() {
            let id = ImportName::latin(name).id("weapon")?;
            let list = |name: &Option<String>| name.as_deref().map(|name| self.list(name));
            let fire = list(&weapon.fire_fx).transpose()?.flatten();
            let hit = list(&weapon.detonation_fx).transpose()?.flatten();
            let look = match delivery(&id) {
                WeaponShot::Dropped => continue,
                WeaponShot::Shot => ActionLook { fire, hit },
                WeaponShot::Projectile(projectile) => {
                    let trail = weapon.exhaust.as_deref().map(|name| {
                        let name = ImportName::latin(name).declared()?;
                        Ok::<_, NameError>(self.file.systems().contains_key(&name).then_some(name))
                    });
                    let trail = trail.transpose()?.flatten();
                    if hit.is_some() || trail.is_some() {
                        let look = units
                            .entry(projectile.clone())
                            .or_insert_with(|| ClientUnit::new(Vec::new()));
                        look.end = hit;
                        look.trail = trail;
                    }
                    ActionLook { fire, hit: None }
                }
            };
            if look != ActionLook::default() {
                looks.insert(id, look);
            }
        }
        Ok(looks)
    }

    /// The package's names of the lists of the game's `names` the file holds, in their order.
    fn lists<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Result<Vec<DeclaredName>, NameError> {
        let mut held = Vec::new();
        for name in names {
            held.extend(self.list(name)?);
        }
        Ok(held)
    }

    /// The package's name of the list of the game's name `name`, when the file holds it.
    fn list(&self, name: &str) -> Result<Option<DeclaredName>, NameError> {
        let name = ImportName::latin(name).declared()?;
        Ok(self.file.lists().contains_key(&name).then_some(name))
    }

    /// The package's name of each of `names`, the game's; an error for two that give one.
    fn names<'a>(
        names: impl Iterator<Item = &'a String>,
    ) -> Result<BTreeMap<&'a str, DeclaredName>, NameError> {
        let mut given = BTreeMap::new();
        let mut taken = BTreeMap::<DeclaredName, &str>::new();
        for name in names {
            let declared = ImportName::latin(name).declared()?;
            if let Some(first) = taken.insert(declared.clone(), name) {
                return Err(NameError::Clash(NameClash {
                    first: first.to_owned(),
                    second: name.clone(),
                }));
            }
            given.insert(name.as_str(), declared);
        }
        Ok(given)
    }

    /// The cause of a nugget of `kind`.
    const fn nugget_cause(kind: NuggetKind) -> EffectDropCause {
        match kind {
            NuggetKind::Sound => EffectDropCause::Sound,
            NuggetKind::RayEffect => EffectDropCause::RayEffect,
            NuggetKind::Tracer => EffectDropCause::Tracer,
            NuggetKind::LightPulse => EffectDropCause::LightPulse,
            NuggetKind::ViewShake => EffectDropCause::ViewShake,
            NuggetKind::TerrainScorch => EffectDropCause::TerrainScorch,
            NuggetKind::FxListAtBonePos => EffectDropCause::AtBone,
        }
    }

    /// The entry of a list that `nugget` makes, of a system `names` gives; none for a count
    /// below 1, which starts nothing.
    fn entry(
        nugget: &ParticleNugget,
        names: &BTreeMap<&str, DeclaredName>,
    ) -> Option<ListedSystem> {
        let count = u32::try_from(nugget.count)
            .ok()
            .filter(|&count| count > 0)?;
        // The game's rotations, about its x, its y and its z in turn, each of a local frame, as
        // one rotation on the engine's axes: its y the engine's −z, its z the engine's y.
        let [x, y, z] = nugget.rotate;
        let rotation =
            Quat::from_rotation_x(x) * Quat::from_rotation_z(-y) * Quat::from_rotation_y(z);
        let steps = |spread: Spread| Spread {
            low: spread.low * STEPS_PER_MS,
            high: spread.high * STEPS_PER_MS,
        };
        Some(ListedSystem {
            system: names[nugget.name.as_str()].clone(),
            count,
            offset: Vec3::from_array(axes::y_up(nugget.offset)),
            radius: nugget.radius,
            height: nugget.height,
            delay: (nugget.delay.high >= 0.0).then(|| steps(nugget.delay)),
            rotation,
            orient_to_object: nugget.orient_to_object,
            attach_to_object: nugget.attach_to_object,
            at_ground: nugget.at_ground,
            callers_radius: nugget.callers_radius,
            ricochet: nugget.ricochet,
        })
    }

    /// The engine's system of `system`, of the texture `texture`, its slave's particles
    /// `slave_offset` off its own, each system it names by its name `named` gives, none for one
    /// it drops.
    fn system(
        system: &IniParticleSystem,
        texture: &PackagePath,
        slave_offset: [f32; 3],
        mut named: impl FnMut(&str) -> Option<DeclaredName>,
    ) -> ParticleSystem {
        let vector = |vector: [f32; 3]| Vec3::from_array(axes::y_up(vector));
        // A box's half sizes are lengths, which the turn of the axes keeps of their sign.
        let [x, y, z] = system.box_half;
        let volume = match system.volume {
            VolumeKind::None | VolumeKind::Point => EmissionVolume::Point,
            VolumeKind::Line => EmissionVolume::Line {
                start: vector(system.line_start),
                end: vector(system.line_end),
            },
            VolumeKind::Box => EmissionVolume::Box {
                half: Vec3::new(x, z, y),
            },
            VolumeKind::Sphere => EmissionVolume::Sphere {
                radius: system.sphere_radius,
            },
            VolumeKind::Cylinder => EmissionVolume::Cylinder {
                radius: system.cylinder_radius,
                length: system.cylinder_length,
            },
        };
        let [ortho_x, ortho_y, ortho_z] = system.ortho;
        let velocity = match system.velocity {
            VelocityKind::None => EmissionVelocity::Still,
            VelocityKind::Ortho => EmissionVelocity::Ortho {
                x: ortho_x,
                y: ortho_z,
                z: Spread {
                    low: -ortho_y.high,
                    high: -ortho_y.low,
                },
            },
            VelocityKind::Spherical => EmissionVelocity::Spherical {
                speed: system.spherical,
            },
            VelocityKind::Hemispherical => EmissionVelocity::Hemispherical {
                speed: system.hemispherical,
            },
            VelocityKind::Cylindrical => EmissionVelocity::Cylindrical {
                radial: system.cylindrical_radial,
                normal: system.cylindrical_normal,
            },
            VelocityKind::Outward => EmissionVelocity::Outward {
                speed: system.outward,
                other: system.outward_other,
            },
        };
        let slave = named(&system.slave).map(|slave| SlaveSystem {
            system: slave,
            offset: vector(slave_offset),
        });
        ParticleSystem {
            rank: Self::rank(system.priority),
            blend: match system.shader {
                ParticleShader::Additive => Blend::Add,
                ParticleShader::Alpha => Blend::Alpha,
                ParticleShader::AlphaTest => Blend::Mask { cutoff: CUTOFF },
                ParticleShader::Multiply => Blend::Multiply,
                ParticleShader::None => unreachable!("a system of no shader is dropped"),
            },
            texture: texture.clone(),
            ground_aligned: system.ground_aligned,
            up_towards_emitter: system.up_towards_emitter,
            above_ground_only: system.above_ground_only,
            angle: system.angle_z,
            angular_rate: system.angular_rate_z,
            angular_damping: system.angular_damping,
            velocity_damping: system.velocity_damping,
            gravity: system.gravity,
            drift: vector(system.drift),
            lifetime: system.lifetime,
            system_lifetime: system.system_lifetime,
            size: system.size,
            start_size_rate: system.start_size_rate,
            size_rate: system.size_rate,
            size_rate_damping: system.size_rate_damping,
            alpha: Self::keys(&system.alpha)
                .map(|key| AlphaKey {
                    alpha: key.value,
                    at: key.frame,
                })
                .collect(),
            color: Self::keys(&system.color)
                .map(|key| ColorKey {
                    color: key.value,
                    at: key.frame,
                })
                .collect(),
            color_scale: system.color_scale,
            burst_delay: system.burst_delay,
            burst_count: system.burst_count,
            initial_delay: system.initial_delay,
            velocity,
            volume,
            hollow: system.hollow,
            slave,
            attached: named(&system.attached),
        }
    }

    /// The rank of `priority` under the cap, the game's order from its lowest; none for
    /// `ALWAYS_RENDER`, which passes the cap.
    fn rank(priority: ParticlePriority) -> Option<u8> {
        (priority != ParticlePriority::AlwaysRender).then_some(priority as u8)
    }

    /// The keys the game runs of `keys`: the first, then each up to the first of frame 0 after
    /// it, which ends them (`Particle::update`); in the order of their frames, the last of each
    /// frame kept, so each runs to a later one.
    fn keys<T: Copy>(keys: &[IniKey<T>; KEYS]) -> impl Iterator<Item = IniKey<T>> {
        let ended = keys[1..].iter().position(|key| key.frame == 0);
        let mut run = keys[..ended.map_or(KEYS, |at| at + 1)].to_vec();
        run.sort_by_key(|key| key.frame);
        let mut kept: Vec<IniKey<T>> = Vec::with_capacity(run.len());
        for key in run {
            match kept.last_mut() {
                Some(last) if last.frame == key.frame => *last = key,
                _ => kept.push(key),
            }
        }
        kept.into_iter()
    }
}

#[cfg(test)]
mod tests;
