use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_math::Vec3;
use campfire_sim::{Capability, Position, SimSet, StableId};

use crate::combat::CombatSet;
use crate::declare::declarations::Declarations;
use crate::declare::install_cx::InstallCx;
use crate::declare::module::Module;
use crate::declare::scratch::Scratch;
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

impl Module for Deliveries {
    const CAPABILITY: Option<Capability> = None;
    const NEEDS: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.scratch::<Deliveries>();
        d.api(DeliveriesApi::register);
    }

    /// Adds the deliveries' hooks to a match of a capability that delivers: after the
    /// `DeliverySet`s, before attacks pay their toggles and strike.
    fn install(cx: &mut InstallCx<'_>) {
        let delivers = [
            Capability::Projectiles,
            Capability::Areas,
            Capability::Abilities,
        ];
        if !delivers.into_iter().any(|capability| cx.has(capability)) {
            return;
        }
        cx.world.insert_resource(Deliverers::default());
        cx.schedule.configure_sets(
            (DeliverySet::Fly, DeliverySet::Trigger)
                .chain()
                .in_set(SimSet::Hit)
                .before(CombatSet::Pay),
        );
        if cx.scripts() {
            cx.schedule.add_systems(
                DeliveryHooks::deliver
                    .in_set(SimSet::Hit)
                    .after(DeliverySet::Trigger)
                    .before(CombatSet::Pay),
            );
        } else {
            cx.schedule.add_systems(
                DeliveryHooks::end
                    .in_set(SimSet::Hit)
                    .after(DeliverySet::Trigger)
                    .before(CombatSet::Pay),
            );
        }
    }
}

impl Deliveries {
    /// Records the end of a dash that delivers `dash`, aimed at `target` if a unit, whose unit
    /// stands at `pos` after a last step in `direction`: an end with no delivery unit, after
    /// the meters the dash went, whose hooks are its action's.
    pub(crate) fn end_dash(
        &mut self,
        dash: DashDelivery,
        target: Option<StableId>,
        pos: Position,
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
            pos,
            distance: dash.dashed,
            direction,
        };
        self.delivered.push(Delivered {
            by,
            reach: Reached::End,
            hit,
        });
    }
}

impl Scratch for Deliveries {
    fn is_clear(&self) -> bool {
        self.delivered.is_empty() && self.ended.is_empty()
    }
}
