use std::collections::BTreeMap;
use std::f32::consts::PI;

use campfire_client_files::{
    ClientUnit, DeathFall, DeathLook, DeathWrecks, FallStage, GroundDeath, SelfSpin, Spiral, Wreck,
    WreckLaunch, WreckList, WreckSink,
};
use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_values::PackagePath;

use crate::zero_hour::axes;
use crate::zero_hour::dropped::{Dropped, Losses};
use crate::zero_hour::effect_drop_cause::EffectDropCause;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::game_frames;
use crate::zero_hour::format::ini::creation_list_ini::{Creation, CreationListIni, CreationNugget};
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::object_ini::ini_fall::{FallKind, IniFall};
use crate::zero_hour::format::ini::object_ini::ini_object::IniObject;
use crate::zero_hour::game_path;
use crate::zero_hour::game_time;
use crate::zero_hour::type_name::TypeName;

/// The wrecks of the install's deaths, as each look names them (02-engine-core.md, A death names
/// its wrecks by phase; 02-engine-core.md, An aircraft's death falls as a wreck): the hulks and the
/// debris of the creation lists each phase of a slow death names, and an aircraft's fall with its
/// own; each model a wreck draws converted as a look's; and what it leaves, counted.
#[derive(Debug)]
pub(crate) struct WreckImport<'a> {
    objects: &'a ObjectIni,
    lists: &'a CreationListIni,
    locomotors: &'a LocomotorIni,
    /// How a piece of debris sinks once it rests, as the game's `GenericDebris` does.
    debris: WreckSink,
    /// How high above the ground a jet dies on it, as `Thing::isSignificantlyAboveTerrain`
    /// tells: 9 frames squared of the game's gravity.
    ground_height: Num,
    losses: Losses<EffectDropCause>,
}

/// What a creation list makes of its nuggets: its wrecks and its nuggets' losses, by the list's
/// name.
#[derive(Debug, Default)]
struct Converted {
    lists: BTreeMap<String, WreckList>,
}

/// The game's object a `CreateDebris` makes, whose slow death a resting piece takes.
const DEBRIS: &str = "GenericDebris";

impl<'a> WreckImport<'a> {
    /// The import of the deaths of `objects`, of the creation lists `lists`, whose aircraft fly by
    /// `locomotors`, under the game's `gravity` in world units a second squared.
    pub(crate) fn new(
        objects: &'a ObjectIni,
        lists: &'a CreationListIni,
        locomotors: &'a LocomotorIni,
        gravity: f32,
    ) -> Result<Self, ZeroHourError> {
        let generic = objects.named(DEBRIS).and_then(IniObject::slow_death);
        let debris = match generic {
            Some(death) => WreckSink {
                life: None,
                delay: Self::seconds(DEBRIS, death.sink_delay_ms)?,
                rate: Self::num(DEBRIS, death.sink_rate)?,
                gone: Self::seconds(DEBRIS, death.delay_ms)?,
            },
            None => WreckSink {
                life: None,
                delay: Num::ZERO,
                rate: Num::ZERO,
                gone: Num::ZERO,
            },
        };
        let frame = gravity / (game_frames::PER_SECOND * game_frames::PER_SECOND);
        Ok(Self {
            objects,
            lists,
            locomotors,
            debris,
            ground_height: Self::num("GameData", -9.0 * frame)?,
            losses: Losses::default(),
        })
    }

    /// Names in `looks` the wrecks each phase of each object's death throws, and its fall, each
    /// model as `written` writes it, which gives whether it draws; what it leaves, counted, given
    /// back.
    pub(crate) fn name(
        mut self,
        written: &mut dyn FnMut(&str) -> Result<bool, ZeroHourError>,
        looks: &mut BTreeMap<DeclaredName, ClientUnit>,
    ) -> Result<Vec<Dropped<EffectDropCause>>, ZeroHourError> {
        let mut converted = Converted::default();
        for object in self.objects.objects() {
            let Some(look) = looks.get_mut(TypeName::of(object)?.declared()) else {
                continue;
            };
            let mut death = look.death.clone();
            match self.throw(object, &mut death, written, &mut converted) {
                Ok(()) => look.death = death,
                // A number of one object's wrecks the engine cannot hold drops its wrecks and its
                // fall alone (09-zero-hour.md, A fault of one object drops its part).
                Err(ZeroHourError::Wreck { .. }) => {
                    self.losses.drop(&object.name, EffectDropCause::Number);
                }
                Err(error) => return Err(error),
            }
        }
        Ok(self.losses.into_iter().collect())
    }

    /// Writes into `death` the wrecks each phase of `object`'s death throws, and its fall.
    fn throw(
        &mut self,
        object: &IniObject,
        death: &mut DeathLook,
        written: &mut dyn FnMut(&str) -> Result<bool, ZeroHourError>,
        converted: &mut Converted,
    ) -> Result<(), ZeroHourError> {
        let mut wrecks = DeathWrecks::NONE;
        if let Some(phases) = object.slow_death().map(|death| &death.phases) {
            let [start, middle, end] = phases.each_ref().map(|phase| &phase.objects);
            wrecks.start = self.lists(&object.name, start, written, converted)?;
            wrecks.middle = self.lists(&object.name, middle, written, converted)?;
            wrecks.end = self.lists(&object.name, end, written, converted)?;
        }
        if let Some(fall) = object.fall() {
            death.fall = Some(self.fall(object, fall, written, converted)?);
        }
        death.wrecks = wrecks;
        Ok(())
    }

    /// The fall of `fall`, of `object`, its stages' wrecks alone, their effects named after; a
    /// helicopter's spiral of its first normal locomotor that hovers, which brakes it as it falls.
    fn fall(
        &mut self,
        object: &IniObject,
        fall: &IniFall,
        written: &mut dyn FnMut(&str) -> Result<bool, ZeroHourError>,
        converted: &mut Converted,
    ) -> Result<DeathFall, ZeroHourError> {
        let name = object.name.as_str();
        if fall.left.effect.is_some() || fall.left.objects.is_some() {
            self.losses.drop(name, EffectDropCause::FallMoment);
        }
        let height = self.ground_height;
        let mut stage = |objects: &Option<String>| -> Result<FallStage, ZeroHourError> {
            let objects: Vec<String> = objects.iter().cloned().collect();
            Ok(FallStage {
                effects: Vec::new(),
                wrecks: self.lists(name, &objects, written, converted)?,
            })
        };
        let start = stage(&fall.start.objects)?;
        let hit = stage(&fall.hit.objects)?;
        let blow_up = stage(&fall.blow_up.objects)?;
        let ground = match fall.kind {
            FallKind::Jet => Some(GroundDeath {
                height,
                stage: stage(&fall.ground.objects)?,
            }),
            FallKind::Helicopter => None,
        };
        let spiral = match fall.kind {
            FallKind::Jet => None,
            FallKind::Helicopter => Some(self.spiral(object, fall)?),
        };
        Ok(DeathFall {
            gravity: Self::num(name, fall.gravity)?,
            roll: Self::num(name, fall.roll * game_frames::PER_SECOND)?,
            spiral,
            start,
            hit,
            blow_up_after: Self::num(name, fall.blow_up_ms / 1000.0)?,
            blow_up,
            ground,
        })
    }

    /// The spiral of the helicopter `object`'s `fall`, as `HelicopterSlowDeathBehavior` pushes it
    /// (02-engine-core.md, An aircraft's death falls as a wreck): its push, a velocity a frame
    /// over its `Mass` each frame, as `PhysicsBehavior::applyForce` takes a force, in metres a
    /// second squared; its turn and its spin's rates in radians a second, its spin's change in
    /// radians a second, as each change adds its amount over the frames of a second, and its delay
    /// in seconds; its fade a frame, the effects' step; and its hover's brake, the least of the
    /// locomotor's `Braking` and the death's `MaxBraking`, and its `AccelerationDamaged`, as a
    /// dead unit's body is past every damage state, none of no locomotor that hovers.
    fn spiral(&self, object: &IniObject, fall: &IniFall) -> Result<Spiral, ZeroHourError> {
        let name = object.name.as_str();
        let hover = object
            .locomotors()
            .iter()
            .filter_map(|locomotor| self.locomotors.get(locomotor))
            .find(|locomotor| locomotor.hovers());
        let (braking, acceleration) = hover.map_or((0.0, 0.0), |hover| {
            (
                hover.braking().min(fall.braking),
                hover.acceleration_damaged(),
            )
        });
        let radians = |degrees: f32| Self::num(name, degrees.to_radians());
        Ok(Spiral {
            push: Self::num(name, fall.speed * game_frames::PER_SECOND / object.mass())?,
            turn: radians(fall.turn)?,
            fade: Self::num(name, fall.damping)?,
            braking: Self::num(name, braking)?,
            acceleration: Self::num(name, acceleration)?,
            spin: SelfSpin {
                least: radians(fall.spin[0])?,
                most: radians(fall.spin[1])?,
                change: radians(fall.spin_change)?,
                every: Self::seconds(name, fall.spin_delay_ms)?,
            },
        })
    }

    /// The wreck lists of the creation lists `names`, the object `name`'s; each list of no wreck
    /// left out, and each name of no list counted.
    fn lists(
        &mut self,
        name: &str,
        names: &[String],
        written: &mut dyn FnMut(&str) -> Result<bool, ZeroHourError>,
        converted: &mut Converted,
    ) -> Result<Vec<WreckList>, ZeroHourError> {
        let mut lists = Vec::new();
        for list in names {
            if list.eq_ignore_ascii_case("none") {
                continue;
            }
            if !converted.lists.contains_key(list) {
                let Some(nuggets) = self.lists.lists().get(list) else {
                    self.losses.drop(name, EffectDropCause::NoCreationList);
                    continue;
                };
                let wrecks = self.list(list, nuggets, written)?;
                converted.lists.insert(list.clone(), wrecks);
            }
            let wrecks = &converted.lists[list];
            if !wrecks.is_empty() {
                lists.push(wrecks.clone());
            }
        }
        Ok(lists)
    }

    /// The wrecks of the creation list `name` of `nuggets`: each hulk an object of it makes, and
    /// each debris; what else it makes counted.
    fn list(
        &mut self,
        name: &str,
        nuggets: &[CreationNugget],
        written: &mut dyn FnMut(&str) -> Result<bool, ZeroHourError>,
    ) -> Result<WreckList, ZeroHourError> {
        let mut wrecks = Vec::new();
        for nugget in nuggets {
            match nugget {
                CreationNugget::Debris(creation) => {
                    let names = creation
                        .names
                        .iter()
                        .map(|model| model.to_ascii_lowercase());
                    let drawn = self.models(name, names, written)?;
                    let sink = self.debris;
                    if let Some(wreck) = self.wreck(name, creation, drawn, creation.mass, sink)? {
                        wrecks.push(wreck);
                    }
                }
                CreationNugget::Objects(creation) => {
                    for object in &creation.names {
                        let hulk = self.objects.named(object).filter(|object| object.is_hulk());
                        let Some(hulk) = hulk else {
                            self.losses.drop(name, EffectDropCause::CreationObject);
                            continue;
                        };
                        let model = hulk.draws().iter().find_map(|draw| draw.model.clone());
                        let drawn = self.models(name, model.into_iter(), written)?;
                        let sink = self.hulk_sink(hulk)?;
                        if let Some(wreck) = self.wreck(name, creation, drawn, hulk.mass(), sink)? {
                            wrecks.push(wreck);
                        }
                    }
                }
                CreationNugget::Other(_) => self.losses.drop(name, EffectDropCause::CreationNugget),
            }
        }
        Ok(WreckList(wrecks))
    }

    /// How the hulk `hulk` ends: after the mean of its lifetime, by its own slow death's sink.
    fn hulk_sink(&self, hulk: &IniObject) -> Result<WreckSink, ZeroHourError> {
        let name = &hulk.name;
        let [least, most] = hulk.lifetime_ms().unwrap_or_default();
        let death = hulk.slow_death();
        Ok(WreckSink {
            life: Some(Self::seconds(name, f32::midpoint(least, most))?),
            delay: Self::seconds(name, death.map_or(0.0, |death| death.sink_delay_ms))?,
            rate: Self::num(name, death.map_or(0.0, |death| death.sink_rate))?,
            gone: Self::seconds(name, death.map_or(0.0, |death| death.delay_ms))?,
        })
    }

    /// The package paths of the models `names`, each as `written` writes it as a look's, which
    /// gives whether it draws; one that draws nothing counted.
    fn models(
        &mut self,
        name: &str,
        names: impl Iterator<Item = String>,
        written: &mut dyn FnMut(&str) -> Result<bool, ZeroHourError>,
    ) -> Result<Vec<PackagePath>, ZeroHourError> {
        let mut drawn = Vec::new();
        for model in names {
            if written(&model)? {
                drawn.push(game_path::model(&model)?);
            } else {
                self.losses.drop(name, EffectDropCause::WreckModel);
            }
        }
        Ok(drawn)
    }

    /// The wreck of `creation` of the models `models`, of `mass`, ending by `sink`; none of no
    /// model. A launch the import leaves is counted, and the rest of the creation kept.
    fn wreck(
        &mut self,
        name: &str,
        creation: &Creation,
        models: Vec<PackagePath>,
        mass: f32,
        sink: WreckSink,
    ) -> Result<Option<Wreck>, ZeroHourError> {
        if models.is_empty() {
            return Ok(None);
        }
        let disposition = creation.disposition;
        if disposition.floating || disposition.whirling || disposition.inherit_velocity {
            self.losses.drop(name, EffectDropCause::WreckLaunch);
        }
        let intensity = creation.intensity;
        // A force over the mass is one logic frame's change of velocity
        // (`PhysicsBehavior::applyForce`), in world units a frame.
        let speed = |force: f32| {
            Self::num(
                name,
                force / mass.max(f32::MIN_POSITIVE) * game_frames::PER_SECOND,
            )
        };
        let launch = if disposition.send_it_flying {
            WreckLaunch::Fling {
                across: speed(4.0 * intensity)?,
                up: [speed(3.0 * intensity * 0.33)?, speed(3.0 * intensity)?],
            }
        } else if disposition.send_it_up {
            WreckLaunch::Fling {
                across: speed(2.0 * intensity)?,
                up: [speed(4.0 * intensity * 0.75)?, speed(4.0 * intensity)?],
            }
        } else if disposition.random_force {
            WreckLaunch::Force {
                speed: [speed(creation.force[0])?, speed(creation.force[1])?],
                pitch: [
                    Self::num(name, creation.pitch[0].to_radians())?,
                    Self::num(name, creation.pitch[1].to_radians())?,
                ],
            }
        } else if disposition.send_it_out {
            WreckLaunch::Out {
                across: speed(4.0 * intensity)?,
            }
        } else if disposition.like_existing {
            WreckLaunch::Still
        } else {
            WreckLaunch::Grounded
        };
        // A spin rate of none is π/32 a frame times the intensity.
        let spin = creation.spin.map_or(
            PI / 32.0 * intensity * game_frames::PER_SECOND,
            f32::to_radians,
        );
        let offset = axes::y_up(creation.offset);
        Ok(Some(Wreck {
            models,
            count: u8::try_from(creation.count.clamp(0, i32::from(u8::MAX)))
                .expect("a count clamped"),
            offset: [
                Self::num(name, offset[0])?,
                Self::num(name, offset[1])?,
                Self::num(name, offset[2])?,
            ],
            launch,
            spin: Self::num(name, spin)?,
            sink,
        }))
    }

    /// The seconds of the game's frames that `ms` of the wreck of `name` take, as the sim's death
    /// delay of the same time counts them (09-zero-hour.md, Thirty frames a second, the original's
    /// times), so a wreck sinks and its unit goes on one frame; an error for a time the game takes
    /// no frames of.
    fn seconds(name: &str, ms: f32) -> Result<Num, ZeroHourError> {
        let frames = game_time::of_duration(ms).map_err(|_| ZeroHourError::Wreck {
            name: name.to_owned(),
        })?;
        Self::num(name, frames as f32 / 1000.0)
    }

    /// `value` of the wreck of `name` as a number; an error for one not finite or past a number.
    fn num(name: &str, value: f32) -> Result<Num, ZeroHourError> {
        game_float::to_num(value).map_err(|_| ZeroHourError::Wreck {
            name: name.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests;
