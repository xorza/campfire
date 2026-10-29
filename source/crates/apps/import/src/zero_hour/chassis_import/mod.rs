use std::collections::BTreeMap;

use campfire_client_files::{Chassis, ClientUnit};
use campfire_common::DeclaredName;

use crate::zero_hour::dropped::{Dropped, Losses};
use crate::zero_hour::effect_drop_cause::EffectDropCause;
use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::error::FloatError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::locomotor_ini::{IniChassis, LocomotorIni};
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::type_name::TypeName;

/// The chassis of the install's tracked and wheeled units (02-engine-core.md, A chassis follows
/// the ground on springs).
#[derive(Debug)]
pub(crate) struct ChassisImport;

impl ChassisImport {
    /// Names in `looks` the chassis of each object of `objects` whose first normal locomotor of
    /// `locomotors` that drives has one, its springs a frame's, the effects' step, and its pitch
    /// limits in radians, as `Drawable::calcPhysicsXformTreads` reads them; one of a number the
    /// engine cannot hold goes, counted.
    pub(crate) fn name(
        objects: &ObjectIni,
        locomotors: &LocomotorIni,
        looks: &mut BTreeMap<DeclaredName, ClientUnit>,
    ) -> Result<Vec<Dropped<EffectDropCause>>, ZeroHourError> {
        let mut losses = Losses::default();
        for object in objects.objects() {
            let Some(look) = looks.get_mut(TypeName::of(object)?.declared()) else {
                continue;
            };
            let drives = object
                .locomotors()
                .iter()
                .filter_map(|name| locomotors.get(name))
                .find(|locomotor| locomotor.drives());
            if let Some(chassis) = drives.and_then(|locomotor| locomotor.chassis()) {
                match Self::of(chassis) {
                    Ok(chassis) => look.chassis = Some(chassis),
                    Err(_) => losses.drop(&object.name, EffectDropCause::Number),
                }
            }
        }
        Ok(losses.into_iter().collect())
    }

    /// The chassis of `chassis`.
    fn of(chassis: &IniChassis) -> Result<Chassis, FloatError> {
        let num = game_float::to_num;
        let radians = |degrees: f32| num(degrees.to_radians());
        Ok(Chassis {
            pitch_stiffness: num(chassis.pitch_stiffness)?,
            roll_stiffness: num(chassis.roll_stiffness)?,
            pitch_damping: num(chassis.pitch_damping)?,
            roll_damping: num(chassis.roll_damping)?,
            axial_damping: num(chassis.axial_damping)?,
            forward_pitch: num(chassis.forward_pitch)?,
            lateral_roll: num(chassis.lateral_roll)?,
            accelerating: radians(chassis.accelerating)?,
            braking: radians(chassis.braking)?,
        })
    }
}

#[cfg(test)]
mod tests;
