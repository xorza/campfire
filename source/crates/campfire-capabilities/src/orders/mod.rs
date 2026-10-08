use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{SimSet, StateRegistry};

use crate::abilities::AbilitiesSet;
use crate::actions::ActionsSet;
use crate::combat::CombatSet;
use crate::geometry::bounds::Bounds;
use crate::items::ItemsSet;
use crate::items::item_book::ItemBook;
use crate::orders::ai::Ai;
use crate::orders::destinations::Destinations;
use crate::orders::learn_orders::LearnOrders;
use crate::orders::next_think::NextThink;
use crate::orders::order_inputs::{OrderInputs, PlayerOrders};
use crate::orders::production_orders::ProductionOrders;
use crate::orders::resetting::Resetting;
use crate::orders::thinking::Thinking;
use crate::orders::tick_orders::TickOrders;
use crate::orders::trade_orders::TradeOrders;
use crate::orders::unit_order::{OrderedUnit, UnitOrder};
use crate::production::ProductionSet;
use crate::production::build_specs::BuildSpecs;
use crate::scripts::ctx::Ctx;
use crate::stats::StatsSet;
use crate::units::by_type::ByType;

pub(crate) mod ai;
pub(crate) mod ai_data;
pub(crate) mod destinations;
pub(crate) mod error;
pub(crate) mod learn_orders;
pub(crate) mod learning;
pub(crate) mod next_think;
pub(crate) mod order;
pub(crate) mod order_inputs;
pub(crate) mod orders_api;
pub(crate) mod production_orders;
pub(crate) mod resetting;
pub(crate) mod thinking;
pub(crate) mod tick_orders;
pub(crate) mod trade_orders;
pub(crate) mod unit_order;

/// The systems of `orders`, for the mode to order its own against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OrdersSet {
    /// In `SimSet::Inputs`: the tick's orders become current.
    Orders,
}

/// The `orders` capability: units that take orders from a player, or from the AI script of their
/// type.
#[derive(Debug)]
pub struct Orders;

impl Orders {
    /// Adds orders to a match: in Inputs, the tick's orders are read, orders become current, ranks
    /// are learned, and, with production, trains are cancelled and rally points set, and, with
    /// items, on the server, items trade; in Think, the resets whose units arrived end, then the
    /// units due this tick think; in Act, before combat starts attacks, units walk their paths and
    /// chase their targets. It builds on the core `Units` installs, on combat and on navigation,
    /// and installs after production and items, whose actions it applies only when they are
    /// installed. Without the core's scripts, as on a client, no unit thinks and no item trades.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.init_resource::<PlayerOrders>();
        world.init_resource::<TickOrders>();
        schedule.add_systems((
            (
                OrderInputs::read_orders,
                OrderInputs::check_player_orders,
                OrderInputs::apply_player_orders,
                LearnOrders::learn_ranks,
            )
                .chain()
                .in_set(SimSet::Inputs)
                .in_set(OrdersSet::Orders)
                .after(StatsSet::Regenerate)
                .after(CombatSet::Respawn),
            (Destinations::follow_paths, Destinations::chase)
                .chain()
                .in_set(SimSet::Act)
                .before(CombatSet::Attack),
        ));
        schedule.configure_sets((
            ActionsSet::HoldAtInputs.after(OrdersSet::Orders),
            ItemsSet::HoldAtInputs.after(OrdersSet::Orders),
            ProductionSet::CheckBuilds.after(OrdersSet::Orders),
            OrdersSet::Orders.after(AbilitiesSet::Toggles),
        ));
        registry.register_component::<NextThink>();
        registry.register_component::<Resetting>();
        let production = world.contains_resource::<BuildSpecs>();
        if production {
            schedule.add_systems(
                ProductionOrders::apply_production_orders
                    .in_set(SimSet::Inputs)
                    .in_set(OrdersSet::Orders)
                    .after(LearnOrders::learn_ranks),
            );
        }
        if !world.contains_non_send::<Ctx>() {
            return;
        }
        world.insert_resource(ByType::<Ai>::default());
        schedule.add_systems(
            (Thinking::end_dead_resets, Thinking::think)
                .chain()
                .in_set(SimSet::Think),
        );
        if world.contains_resource::<ItemBook>() {
            let trade = TradeOrders::trade_items
                .in_set(SimSet::Inputs)
                .in_set(OrdersSet::Orders)
                .after(LearnOrders::learn_ranks);
            if production {
                schedule.add_systems(trade.after(ProductionOrders::apply_production_orders));
            } else {
                schedule.add_systems(trade);
            }
        }
    }

    /// Applies `order`, which its source checked, to the unit of `entity` in `now`, as every
    /// order applies; a unit that resets takes none.
    pub(crate) fn apply_order(world: &mut World, entity: Entity, order: UnitOrder, now: Tick) {
        let bounds = *world.resource::<Bounds>();
        let mut unit = world.entity_mut(entity);
        if unit.contains::<Resetting>() {
            return;
        }
        let parts = unit
            .get_components_mut::<OrderedUnit>()
            .expect("an ordered unit stands");
        if order.apply(parts, &bounds, now) {
            unit.insert(Resetting);
        }
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::orders::Orders;
    use crate::orders::ai::Ai;
    use crate::orders::ai_data::AiData;
    use crate::orders::error::AiError;
    use crate::scripts::script_book::ScriptBook;
    use crate::units::by_type::ByType;
    use crate::units::unit_type::UnitType;
    use bevy_ecs::world::World;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    impl Orders {
        /// Gives `unit_type` its AI, with its compiled script: the think period in milliseconds
        /// becomes whole ticks at the match's rate, rounded up, and at least one.
        pub fn load_ai(
            world: &mut World,
            unit_type: UnitType,
            data: &AiData,
            script: ScriptId,
        ) -> Result<(), AiError> {
            let rate = *world.resource::<TickRate>();
            let ai = Ai::of(data, script, world.resource::<ScriptBook>(), rate)?;
            world.resource_mut::<ByType<Ai>>().set(unit_type, ai);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
