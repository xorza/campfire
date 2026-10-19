use bevy_ecs::world::World;
use campfire_actions::ActionTarget;
use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_navigation::Navigation;
use campfire_orders::OrdersAction;
use campfire_production::Production;
use campfire_sim::StableId;
use campfire_sim::internals::SimWorldInternals;
use campfire_units::{BodyForm, BuildTarget, GroundPoint, Owner, SeenBy};
use campfire_values::{Team, TeamSet, UnitType};

use crate::fixtures::navigation::NavigationFixtures;
use crate::order::{Action, Order};

/// The test fixtures of `Production` that need a crate above its own.
pub trait ProductionFixtures {
    /// A unit of `unit_type` owned by `owner`, at the origin.
    fn owned(world: &mut World, unit_type: UnitType, owner: PlayerSlot) -> StableId;

    /// Runs a tick in which player `slot` orders `action` to `id`.
    fn tick_ordering(world: &mut World, slot: u32, id: StableId, action: impl Into<Action>);

    /// Runs a tick in which player `slot` gives `orders`, each an action to a unit, in order.
    fn give_orders<A: Into<Action> + Copy>(world: &mut World, slot: u32, orders: &[(StableId, A)]);

    /// Runs a tick in which player 0 orders `builder` to build its depot at (`x`, `z`).
    fn build_at(world: &mut World, builder: StableId, x: i64, z: i64);

    /// Runs a tick in which player 0 orders each of `workers` to gather at `node`.
    fn gather_at(world: &mut World, workers: &[StableId], node: StableId);

    /// A walker of `team` at `pos`, of a half meter, seen by `seen`, by all when `None`.
    fn yard_walker(
        world: &mut World,
        pos: impl Into<Vec3>,
        team: Team,
        seen: Option<TeamSet>,
    ) -> StableId;
}

impl ProductionFixtures for Production {
    fn owned(world: &mut World, unit_type: UnitType, owner: PlayerSlot) -> StableId {
        Mode::spawn(
            world,
            unit_type,
            Vec3::ZERO,
            Team::new(1),
            Owner::new(owner),
        )
    }

    fn tick_ordering(world: &mut World, slot: u32, id: StableId, action: impl Into<Action>) {
        Self::give_orders(world, slot, &[(id, action.into())]);
    }

    fn give_orders<A: Into<Action> + Copy>(world: &mut World, slot: u32, orders: &[(StableId, A)]) {
        let orders: Vec<Order> = orders
            .iter()
            .map(|&(unit, action)| Order::one(unit, action))
            .collect();
        let payload = Order::payload(&orders);
        world.step_with(&[(PlayerSlot::new(slot), &payload)]);
    }

    fn build_at(world: &mut World, builder: StableId, x: i64, z: i64) {
        let target = BuildTarget::Point {
            at: GroundPoint {
                x: Num::int(x),
                z: Num::int(z),
            },
            angle: Num::ZERO,
        };
        Self::tick_ordering(world, 0, builder, OrdersAction::Build { slot: 0, target });
    }

    fn gather_at(world: &mut World, workers: &[StableId], node: StableId) {
        let target = ActionTarget::Unit(node);
        let orders: Vec<(StableId, OrdersAction)> = workers
            .iter()
            .map(|&unit| (unit, OrdersAction::Slot { slot: 0, target }))
            .collect();
        Self::give_orders(world, 0, &orders);
    }

    fn yard_walker(
        world: &mut World,
        pos: impl Into<Vec3>,
        team: Team,
        seen: Option<TeamSet>,
    ) -> StableId {
        let form = BodyForm::circle(Num::HALF).unwrap();
        let id = Navigation::spawn_unit(world, pos, team, None, Some(Num::ONE), Some(form));
        if let Some(teams) = seen {
            world.insert_parts(id, SeenBy::new(teams));
        }
        id
    }
}
