use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_math::Vec3;
use campfire_sim::{Capability, Position, SimSet, StableId};

use crate::combat::CombatSet;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::deliveries::delivered::{Delivered, Reached};
use crate::deliveries::deliverers::Deliverers;
use crate::deliveries::deliveries_api::DeliveriesApi;
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

impl Declares for Deliveries {
    const MODULE: Option<Capability> = None;

    fn declare<D: Declarations>(d: &mut D) {
        d.api(DeliveriesApi::register);
    }
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

    /// Adds the deliveries' hooks to a match of a capability that delivers: after the
    /// `DeliverySet`s, before attacks pay their toggles and strike.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        let delivers = [
            Capability::Projectiles,
            Capability::Areas,
            Capability::Abilities,
        ];
        if !delivers.into_iter().any(|capability| cx.has(capability)) {
            return;
        }
        cx.world.insert_resource(Deliveries::default());
        cx.world.insert_resource(Deliverers::default());
        cx.schedule.configure_sets(
            (DeliverySet::Fly, DeliverySet::Trigger)
                .chain()
                .in_set(SimSet::Hit)
                .before(CombatSet::Pay),
        );
        cx.schedule.add_systems(
            DeliveryHooks::deliver
                .in_set(SimSet::Hit)
                .after(DeliverySet::Trigger)
                .before(CombatSet::Pay),
        );
    }
}
