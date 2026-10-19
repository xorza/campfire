use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_navigation::Navigation;
use campfire_sim::StableId;
use campfire_sim::internals::SimWorldInternals;
use campfire_units::{BodyForm, Destination, MoveStep, UnitTypeCore, Units};
use campfire_values::{Team, TypeScope, UnitType};

/// The test fixtures of `Navigation` that need a crate above its own.
pub trait NavigationFixtures {
    /// A unit at `position` walking a meter a tick to `to`.
    fn unit(world: &mut World, position: impl Into<Vec3>, to: Option<Vec3>) -> StableId;

    /// A unit at `position` with a body of `radius`, walking `step` a tick to `to`, or one that
    /// does not walk when `step` is `None`.
    fn body(
        world: &mut World,
        position: impl Into<Vec3>,
        to: Option<Vec3>,
        step: Option<Num>,
        radius: impl Into<Num>,
    ) -> StableId;

    /// A unit at `position` with a body of `form`, walking `step` a tick to `to`, or one that does
    /// not walk when `step` is `None`.
    fn body_on(
        world: &mut World,
        position: impl Into<Vec3>,
        to: Option<Vec3>,
        step: Option<Num>,
        form: BodyForm,
    ) -> StableId;

    /// The mode's type `walker`, of no section, which the navigation tests' units are of:
    /// loaded with the first call, which comes before any unit spawns, with the spawner of
    /// every type.
    fn walker_type(world: &mut World) -> UnitType;

    /// A unit of `team` at `position`, with a body of `form` when given, walking `step` a tick to
    /// `to`, or one that does not walk when `step` is `None`, as `spawn_turned` spawns one.
    fn spawn_unit(
        world: &mut World,
        position: impl Into<Vec3>,
        team: Team,
        to: Option<Vec3>,
        step: Option<Num>,
        form: Option<BodyForm>,
    ) -> StableId;

    /// A unit as `spawn_unit` spawns one, turned `angle` degrees: of the mode's type
    /// `walker`, of no section, which loads before the first unit spawns, its step and body
    /// its own.
    fn spawn_turned(
        world: &mut World,
        position: impl Into<Vec3>,
        angle: Num,
        team: Team,
        to: Option<Vec3>,
        step: Option<Num>,
        form: Option<BodyForm>,
    ) -> StableId;
}

impl NavigationFixtures for Navigation {
    fn unit(world: &mut World, position: impl Into<Vec3>, to: Option<Vec3>) -> StableId {
        Self::spawn_unit(world, position, Team::new(0), to, Some(Num::ONE), None)
    }

    fn body(
        world: &mut World,
        position: impl Into<Vec3>,
        to: Option<Vec3>,
        step: Option<Num>,
        radius: impl Into<Num>,
    ) -> StableId {
        let form = BodyForm::circle(radius.into()).unwrap();
        Self::body_on(world, position, to, step, form)
    }

    fn body_on(
        world: &mut World,
        position: impl Into<Vec3>,
        to: Option<Vec3>,
        step: Option<Num>,
        form: BodyForm,
    ) -> StableId {
        Self::spawn_unit(world, position, Team::new(0), to, step, Some(form))
    }

    fn walker_type(world: &mut World) -> UnitType {
        Units::type_named(world, "walker").unwrap_or_else(|| {
            let core = UnitTypeCore::default();
            Units::load_type(world, TypeScope::Mode, "walker", &core)
        })
    }

    fn spawn_unit(
        world: &mut World,
        position: impl Into<Vec3>,
        team: Team,
        to: Option<Vec3>,
        step: Option<Num>,
        form: Option<BodyForm>,
    ) -> StableId {
        Self::spawn_turned(world, position, Num::ZERO, team, to, step, form)
    }

    fn spawn_turned(
        world: &mut World,
        position: impl Into<Vec3>,
        angle: Num,
        team: Team,
        to: Option<Vec3>,
        step: Option<Num>,
        form: Option<BodyForm>,
    ) -> StableId {
        let unit_type = Self::walker_type(world);
        let id = match form {
            Some(form) => {
                Mode::spawn_turned(world, unit_type, position, angle, team, form.at(angle))
            }
            None => Mode::spawn_turned(world, unit_type, position, angle, team, ()),
        };
        if let Some(step) = step {
            world.insert_parts(id, Self::walker(MoveStep::new(step).unwrap()));
            *world.part_mut::<Destination>(id) = Destination::to(to);
        }
        id
    }
}
