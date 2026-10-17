use bevy_ecs::resource::Resource;
use bevy_ecs::system::{In, SystemId};
use campfire_declare::OneShot;
use campfire_math::Vec3;
use campfire_values::UnitType;

/// Where a unit of a type that walks may stand nearest a point, as the capability that plans the
/// walks answers it: the system it registers, which a spawn runs out of the schedule, so a
/// spawn names no capability of walks. A match with none holds no such run.
#[derive(Resource, Debug, Clone, Copy)]
pub struct OpenCellRun(pub SystemId<In<OpenCellAsk>, Option<Vec3>>);

/// What `OpenCellRun` is asked: the type of the unit, and the point it stands nearest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenCellAsk {
    pub unit_type: UnitType,
    pub near: Vec3,
}

impl OneShot for OpenCellRun {
    type In = In<OpenCellAsk>;
    type Out = Option<Vec3>;

    fn new(id: SystemId<In<OpenCellAsk>, Option<Vec3>>) -> Self {
        Self(id)
    }
}
