use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_sim::{Position, SimSet, StableId};

use crate::combat::CombatSet;
use crate::deliveries::delivered::{Delivered, Reached};
use crate::deliveries::deliverers::Deliverers;
use crate::deliveries::delivering::Delivering;
use crate::deliveries::delivery_hooks::DeliveryHooks;
use crate::units::forced_move::DashDelivery;
use crate::values::hit::Hit;

pub(crate) mod delivered;
pub(crate) mod deliverers;
pub(crate) mod deliveries_api;
pub(crate) mod delivering;
pub(crate) mod delivery_hooks;
pub(crate) mod delivery_spawner;

/// The hits and ends of this tick's deliveries whose action's hooks run, in the order they
/// happened; and the delivery units that ended, which despawn once the hooks ran, so a hook still
/// reads its delivery. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Deliveries {
    pub(crate) delivered: Vec<Delivered>,
    pub(crate) ended: Vec<Entity>,
}

/// The systems that move the deliveries of actions and find what they reach, in Hit before
/// attacks strike, in this order; their hooks run after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DeliverySet {
    /// Projectiles fly.
    Fly,
    /// Areas trigger and end.
    Trigger,
}

impl Deliveries {
    /// Records the end of a dash that delivers `dash`, aimed at `target` if a unit, whose unit
    /// stands at `place` after a last step in `direction`: an end with no delivery unit, after
    /// the meters the dash went, whose hooks are its action's.
    pub(crate) fn end_dash(
        &mut self,
        dash: DashDelivery,
        target: Option<StableId>,
        place: Position,
        direction: Option<Vec3>,
    ) {
        let by = Delivering {
            source: dash.source,
            action: dash.action,
            rank: dash.rank,
            start: dash.start,
            launch: None,
        };
        let hit = Hit {
            delivery: None,
            target,
            pos: place,
            distance: dash.dashed,
            direction,
        };
        self.delivered.push(Delivered {
            by,
            reach: Reached::End,
            hit,
        });
    }

    /// Adds the deliveries' hooks to a match, once for the capabilities that deliver: after the
    /// `DeliverySet`s, before attacks pay their toggles and strike.
    pub(crate) fn install(world: &mut World, schedule: &mut Schedule) {
        if world.contains_resource::<Deliveries>() {
            return;
        }
        world.insert_resource(Deliveries::default());
        world.insert_resource(Deliverers::default());
        schedule.configure_sets(
            (DeliverySet::Fly, DeliverySet::Trigger)
                .chain()
                .in_set(SimSet::Hit)
                .before(CombatSet::Pay),
        );
        schedule.add_systems(
            DeliveryHooks::deliver
                .in_set(SimSet::Hit)
                .after(DeliverySet::Trigger)
                .before(CombatSet::Pay),
        );
    }
}
