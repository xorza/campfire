use bevy_ecs::world::World;
use campfire_actions::ActionTarget;
use campfire_common::PlayerSlot;
use campfire_math::{Num, Vec3};
use campfire_navigation::Navigation;
use campfire_orders::OrdersDirective;
use campfire_production::Production;
use campfire_sim::StableId;
use campfire_sim::internals::SimWorldInternals;
use campfire_units::internals::Spawn;
use campfire_units::{BodyForm, BodyShape, BuildTarget, GroundPoint, Layer, SeenBy};
use campfire_values::{Team, TeamSet, UnitType};

use crate::fixtures::navigation::NavigationFixtures;
use crate::order::{Directive, Order};

/// The test fixtures of `Production` that need a crate above its own.
pub trait ProductionFixtures {
    /// A unit of `unit_type` owned by `owner`, at the origin.
    fn owned(world: &mut World, unit_type: UnitType, owner: PlayerSlot) -> StableId;

    /// Runs a tick in which player `player_slot` orders `directive` to `id`.
    fn tick_ordering(
        world: &mut World,
        player_slot: u32,
        id: StableId,
        directive: impl Into<Directive>,
    );

    /// Runs a tick in which player `player_slot` gives `orders`, each an directive to a unit, in
    /// order.
    fn give_orders<A: Into<Directive> + Copy>(
        world: &mut World,
        player_slot: u32,
        orders: &[(StableId, A)],
    );

    /// Runs a tick in which player 0 orders `builder` to build its depot at (`x`, `z`).
    fn build_at(world: &mut World, builder: StableId, x: i64, z: i64);

    /// Runs a tick in which player 0 orders each of `workers` to gather at `node`.
    fn gather_at(world: &mut World, workers: &[StableId], node: StableId);

    /// A walker of `team` at `position`, of a half meter, seen by `seen`, by all when `None`.
    fn yard_walker(
        world: &mut World,
        position: impl Into<Vec3>,
        team: Team,
        seen: Option<TeamSet>,
    ) -> StableId;
}

impl ProductionFixtures for Production {
    fn owned(world: &mut World, unit_type: UnitType, owner: PlayerSlot) -> StableId {
        Spawn::of(unit_type, Vec3::ZERO)
            .team(Team::new(1))
            .owner(owner)
            .place(world)
    }

    fn tick_ordering(world: &mut World, slot: u32, id: StableId, directive: impl Into<Directive>) {
        Self::give_orders(world, slot, &[(id, directive.into())]);
    }

    fn give_orders<A: Into<Directive> + Copy>(
        world: &mut World,
        slot: u32,
        orders: &[(StableId, A)],
    ) {
        let orders: Vec<Order> = orders
            .iter()
            .map(|&(unit, directive)| Order::one(unit, directive))
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
        Self::tick_ordering(
            world,
            0,
            builder,
            OrdersDirective::Build { slot: 0, target },
        );
    }

    fn gather_at(world: &mut World, workers: &[StableId], node: StableId) {
        let target = ActionTarget::Unit(node);
        let orders: Vec<(StableId, OrdersDirective)> = workers
            .iter()
            .map(|&unit| (unit, OrdersDirective::Slot { slot: 0, target }))
            .collect();
        Self::give_orders(world, 0, &orders);
    }

    fn yard_walker(
        world: &mut World,
        position: impl Into<Vec3>,
        team: Team,
        seen: Option<TeamSet>,
    ) -> StableId {
        let form = BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), Layer::FIRST);
        let id = Navigation::walker_at(world, position)
            .team(team)
            .body(form)
            .walks(Num::ONE, None)
            .place(world);
        if let Some(teams) = seen {
            world.insert_parts(id, SeenBy::new(teams));
        }
        id
    }
}
