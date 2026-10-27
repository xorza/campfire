use std::collections::BTreeMap;

use campfire_client_files::{Chassis, ClientUnit};
use campfire_common::DeclaredName;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::locomotor_ini::{IniChassis, LocomotorIni};
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::import_name::ImportName;

/// The chassis of the install's tracked and wheeled units (02-engine-core.md, A chassis follows
/// the ground on springs).
#[derive(Debug)]
pub(crate) struct ChassisImport;

impl ChassisImport {
    /// Names in `looks` the chassis of each object of `objects` whose first normal locomotor of
    /// `locomotors` that drives has one, its springs a frame's, the effects' step, and its pitch
    /// limits in radians, as `Drawable::calcPhysicsXformTreads` reads them.
    pub(crate) fn name(
        objects: &ObjectIni,
        locomotors: &LocomotorIni,
        looks: &mut BTreeMap<DeclaredName, ClientUnit>,
    ) -> Result<(), ZeroHourError> {
        for object in objects.objects() {
            let Some(look) = looks.get_mut(&ImportName::object(object).declared()?) else {
                continue;
            };
            let drives = object
                .locomotors()
                .iter()
                .filter_map(|name| locomotors.get(name))
                .find(|locomotor| locomotor.drives());
            if let Some(chassis) = drives.and_then(|locomotor| locomotor.chassis()) {
                look.chassis = Some(Self::of(&object.name, chassis)?);
            }
        }
        Ok(())
    }

    /// The chassis of `chassis`, the object `name`'s.
    fn of(name: &str, chassis: &IniChassis) -> Result<Chassis, ZeroHourError> {
        let num = |value: f32| {
            game_float::to_num(value).map_err(|_| ZeroHourError::Chassis {
                name: name.to_owned(),
            })
        };
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
