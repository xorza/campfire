use bevy_ecs::entity::Entity;
use bevy_ecs::world::{EntityRef, World};
use campfire_actions::{ListedAt, ListedSpec};
use campfire_declarations::ById;
use campfire_math::Vec3;
use campfire_script::{CallError, Frame, View};
use campfire_sim::{Position, StableId};
use campfire_units::{
    Dead, Destination, FiringRider, FlightGoal, Life, Owner, SeeingRider, SeenBy, UnitsWorld,
};
use campfire_values::{Share, TagSet, Team, TeamSet, UnitTags, UnitType};

use crate::containers::{ContainerFrame, Containers};
use crate::enter_spec::EnterSpec;
use crate::interaction_effect::InteractionEffect;
use crate::rider_size::RiderSize;
use crate::riders::{HeldFrom, Riders};
use crate::riding::Riding;

/// The effect the `enter` kind ends its `on_resolve` with, which no data writes: its rider enters
/// the container it reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EntryResolve;

impl ListedSpec for EntryResolve {
    fn queue(&self, at: ListedAt, frame: &mut Frame, _: &View) -> Result<(), CallError> {
        frame.queue(InteractionEffect::Enter {
            container: at.unit(frame),
        });
        Ok(())
    }
}

impl Riders {
    /// Puts the rider of `rider` in the container of `container`, when the container takes it;
    /// whether it did. A container takes a living rider of a size, not inside already, that its
    /// filter selects and its room fits, while it lives above its `full` share of life
    /// (transports.md, A container says who rides), of the rider's player, or of a free building
    /// no other player's rider holds (transports.md, A rider may enter its own player's container,
    /// or a free one). The rider leaves the world with its stable id and parts, its team alone
    /// sees it, and it walks nowhere; the container holds the `holding` tag, and a free building
    /// takes its first rider's player and team (transports.md, A free building takes its riders'
    /// side).
    pub(crate) fn enter(world: &mut World, rider: Entity, container: Entity) -> bool {
        let Some(room) = Self::admits(world, rider, container) else {
            return false;
        };
        // A container in the air takes no rider: the rider waits on the ground for it to land
        // (transports.md, A flier lands for its riders).
        if !Self::grounded(world, container) {
            let id = *world
                .get::<StableId>(rider)
                .expect("a rider has a stable id");
            let mut riders = world
                .get_mut::<Self>(container)
                .expect("a container holds its riders");
            riders.wait(id);
            return false;
        }
        let rider_unit = world.entity(rider);
        let id = *rider_unit
            .get::<StableId>()
            .expect("a rider has a stable id");
        let team = *rider_unit.get::<Team>().expect("a rider has a team");
        let owner = rider_unit.get::<Owner>().copied();
        let container_unit = world.entity(container);
        let container_id = *container_unit
            .get::<StableId>()
            .expect("a container has a stable id");
        let held_team = *container_unit
            .get::<Team>()
            .expect("a container has a team");
        let held_owner = container_unit.get::<Owner>().copied();
        let spec = Self::spec(world, container).expect("a container takes riders by its spec");
        let first = world
            .get::<Self>(container)
            .expect("a container holds its riders")
            .is_empty();
        let takes_side = first && spec.capture && held_owner.is_none() && owner.is_some();
        let tags = rider_unit
            .get::<UnitTags>()
            .map_or(TagSet::NONE, |tags| tags.tags);
        let fires = spec.fire.fires(tags);
        let sees = spec.sight;
        let points = spec.fire.points().len();
        let mut riders = world
            .get_mut::<Self>(container)
            .expect("a container holds its riders");
        riders.take(id, room, points);
        let point = riders.iter().last().and_then(|rider| rider.point);
        if takes_side {
            riders.hold_from(HeldFrom {
                owner: held_owner.map(Owner::player_slot),
                team: held_team,
            });
        }
        let points = Self::spec(world, container).map_or(&[][..], |spec| spec.fire.points());
        let place = ContainerFrame::of_unit(world, container).rider_at(points, point);
        let mut unit = world.entity_mut(rider);
        if let Some(mut destination) = unit.get_mut::<Destination>() {
            Destination::route_to(&mut destination, None);
        }
        *unit.get_mut::<Position>().expect("a rider stands") = Position::from(place);
        let riding = (
            Riding::new(container_id),
            SeenBy::new(TeamSet::NONE.with(team)),
        );
        // Out of the world in one move, a rider that fires or sees with its marks.
        match (fires, sees) {
            (true, true) => unit.insert((riding, FiringRider, SeeingRider)),
            (true, false) => unit.insert((riding, FiringRider)),
            (false, true) => unit.insert((riding, SeeingRider)),
            (false, false) => unit.insert(riding),
        };
        if takes_side {
            let owner = owner.expect("a free building takes a player's side");
            world.entity_mut(container).insert((owner, team));
        }
        Containers::holding(world, container, true);
        true
    }

    /// The room the rider of `rider` takes of the container of `container`, when the container
    /// takes it.
    fn admits(world: &World, rider: Entity, container: Entity) -> Option<u32> {
        let spec = Self::spec(world, container)?;
        let (rider_unit, container_unit) = (world.entity(rider), world.entity(container));
        let living = |unit: &EntityRef<'_>| {
            !unit.contains::<Dead>() && unit.get::<Life>().is_none_or(|life| !life.is_empty())
        };
        if rider == container
            || !living(&rider_unit)
            || !living(&container_unit)
            || rider_unit.contains::<Riding>()
        {
            return None;
        }
        let size = rider_unit
            .get::<UnitType>()
            .and_then(|&unit_type| world.resource::<ById<UnitType, RiderSize>>().get(unit_type))?;
        let full = container_unit
            .get::<Life>()
            .is_some_and(|life| Self::at_most(life, spec.full));
        let team = *rider_unit.get::<Team>()?;
        let relation = world
            .relations()
            .between(*container_unit.get::<Team>()?, team);
        let tags = rider_unit
            .get::<UnitTags>()
            .map_or(TagSet::NONE, |tags| tags.tags);
        let riders = container_unit.get::<Self>()?;
        let owner = rider_unit.get::<Owner>();
        let held_by = container_unit.get::<Owner>();
        // A free building of no player takes any rider; one a player holds, by a rider or as its
        // own, takes that player's alone.
        let side = held_by.map_or(spec.capture, |held_by| Some(held_by) == owner);
        let room = spec.room_of(size.get());
        let fits = u64::from(riders.room()) + u64::from(room) <= u64::from(spec.capacity);
        (!full && spec.admits(relation, tags) && side && fits).then_some(room)
    }

    /// Whether the unit of `entity` stands on the ground, as a flier that landed does.
    pub(crate) fn grounded(world: &World, entity: Entity) -> bool {
        let at = world.get::<Position>(entity).expect("a container stands");
        Self::stands(at.get(), world.entity(entity).contains::<FlightGoal>())
    }

    /// Whether a unit at `at`, a flier when it `flies`, stands on the ground: one that does not
    /// fly always, as its height is its map's, and a flier at no height above the ground, as its
    /// flight places it.
    pub(crate) const fn stands(at: Vec3, flies: bool) -> bool {
        !flies || at.y.to_bits() <= 0
    }

    /// The `enter` of the container of `container`'s type.
    fn spec(world: &World, container: Entity) -> Option<&EnterSpec> {
        let unit_type = world.get::<UnitType>(container)?;
        world
            .resource::<ById<UnitType, EnterSpec>>()
            .get(*unit_type)
    }

    /// Whether `life` is at most `share` of its most, as Zero Hour's body is really damaged at
    /// its threshold's ratio and below it (`calcDamageState`); never for no share.
    pub(crate) fn at_most(life: &Life, share: Share) -> bool {
        !share.is_zero() && life.current() <= share.of_num(life.max())
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::Num;

    use super::*;

    #[test]
    fn a_unit_that_does_not_fly_stands_at_any_height_and_a_flier_at_none() {
        // A height of a spatial map's place, 5 m, leaves a walker on the ground; a flier stands
        // at no height above the ground alone.
        let at = |y: i64| Vec3::new(Num::ZERO, Num::int(y), Num::ZERO);
        assert!(Riders::stands(at(5), false));
        assert!(Riders::stands(at(0), true));
        assert!(!Riders::stands(at(1), true));
    }
}
