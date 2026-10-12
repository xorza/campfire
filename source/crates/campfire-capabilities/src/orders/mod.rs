use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{Capability, SimSet};

use crate::abilities::AbilitiesSet;
use crate::actions::ActionsSet;
use crate::combat::CombatSet;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::geometry::bounds::Bounds;
use crate::items::ItemsSet;
use crate::orders::ai::Ai;
use crate::orders::destinations::Destinations;
use crate::orders::learn_orders::LearnOrders;
use crate::orders::next_think::NextThink;
use crate::orders::order_inputs::{OrderInputs, PlayerOrders};
use crate::orders::orders_api::OrdersApi;
use crate::orders::production_orders::ProductionOrders;
use crate::orders::resetting::Resetting;
use crate::orders::thinking::Thinking;
use crate::orders::tick_orders::TickOrders;
use crate::orders::trade_orders::TradeOrders;
use crate::orders::unit_order::{OrderedUnit, UnitOrder};
use crate::production::ProductionSet;
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
pub(crate) mod seen_targets;
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

impl Declares for Orders {
    const MODULE: Option<Capability> = Some(Capability::Orders);

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<NextThink>();
        d.component::<Resetting>();
        d.book::<ByType<Ai>>();
        d.api(OrdersApi::register);
    }
}

impl Orders {
    /// Adds orders to a match: in Inputs, the tick's orders are read, orders become current, ranks
    /// are learned, and, with production, trains are cancelled and rally points set, and, with
    /// items, on the server, items trade; in Think, the resets whose units arrived end, then the
    /// units due this tick think; in Act, before combat starts attacks, units walk their paths and
    /// chase their targets. It builds on the core `Units` installs, on combat and on navigation,
    /// and installs after production and items, whose actions it applies only when they are
    /// installed. Without the core's scripts, as on a client, no unit thinks and no item trades.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        let production = cx.has(Capability::Production);
        let items = cx.has(Capability::Items);
        let scripts = cx.scripts();
        let (world, schedule) = (&mut *cx.world, &mut *cx.schedule);
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
        if production {
            schedule.add_systems(
                ProductionOrders::apply_production_orders
                    .in_set(SimSet::Inputs)
                    .in_set(OrdersSet::Orders)
                    .after(LearnOrders::learn_ranks),
            );
        }
        if !scripts {
            return;
        }
        schedule.add_systems(
            (Thinking::end_dead_resets, Thinking::think)
                .chain()
                .in_set(SimSet::Think),
        );
        if items {
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
    pub(super) fn apply_order(world: &mut World, entity: Entity, order: UnitOrder, now: Tick) {
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
    use bevy_ecs::world::World;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    use crate::orders::Orders;
    use crate::orders::ai::Ai;
    use crate::orders::ai_data::AiData;
    use crate::orders::error::AiError;
    use crate::scripts::script_book::ScriptBook;
    use crate::units::by_type::ByType;
    use crate::units::unit_type::UnitType;

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
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_common::{PlayerSlot, Tick};
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, StableId};

    use crate::actions::action_slots::ActionSlots;
    use crate::actions::in_progress::InProgress;
    use crate::actions::slot_times::SlotTimes;
    use crate::combat::internals::Armed;
    use crate::items::inventory::{Inventory, ItemStack};
    use crate::navigation::Navigation;
    use crate::orders::Orders;
    use crate::orders::ai_data::AiData;
    use crate::players::players_world::PlayersWorld;
    use crate::players::resource_id::ResourceId;
    use crate::scripts::script_failures::ScriptFailures;
    use crate::units::Units;
    use crate::units::move_step::MoveStep;
    use crate::units::owner::Owner;
    use crate::units::team::Team;
    use crate::units::unit_type::UnitType;
    use crate::units::unit_type_data::UnitTypeData;
    use crate::values::declared_name::DeclaredName;
    use crate::values::package_path::PackagePath;
    use crate::values::scalar::Scalar;

    impl Orders {
        /// A unit type of `tags` and `params`, and with the AI script `ai` when it has one, thinking
        /// every 250 ms.
        pub(crate) fn ai_type(
            world: &mut World,
            tags: &[&str],
            params: &[(&str, Scalar)],
            ai: Option<&str>,
        ) -> UnitType {
            let data = UnitTypeData::of(tags, params);
            let unit_type = Units::load_next_type(world, &data);
            if let Some(source) = ai {
                let ai = AiData {
                    ai: PackagePath::parse("scripts/ai.rhai").unwrap(),
                    think_ms: 250,
                };
                let script = Units::compile_hooked(world, source).unwrap();
                Orders::load_ai(world, unit_type, &ai, script).unwrap();
            }
            unit_type
        }

        /// Runs a tick, and checks that no script call failed in it.
        pub(crate) fn think(world: &mut World, inputs: &[(PlayerSlot, &[u8])]) {
            world.step_with(inputs);
            let failures = world.non_send::<ScriptFailures>().calls();
            assert_eq!(failures, []);
        }

        /// When the attack it winds up strikes, if one winds up.
        pub(crate) fn strikes_at(world: &World, id: StableId) -> Option<Tick> {
            world
                .part::<ActionSlots>(id)
                .in_progress()
                .and_then(InProgress::resolves_at)
        }

        /// When its weapon may attack again.
        pub(crate) fn ready_at(world: &World, id: StableId) -> Tick {
            world.part::<SlotTimes>(id).slot(0).unwrap().ready_at
        }

        /// Player 0's gold.
        pub(crate) fn gold(world: &World) -> i64 {
            let resources = world.player_resources(PlayerSlot::new(0));
            let gold = ResourceId::named(&[DeclaredName::new("gold").unwrap()], "gold").unwrap();
            resources.unwrap().amount(gold)
        }

        /// The items of `id`'s inventory, slot by slot, each by its book's index and its count.
        pub(crate) fn carried(world: &World, id: StableId) -> Vec<Option<(usize, u32)>> {
            let inventory = world.try_part::<Inventory>(id).unwrap();
            let carried = |slot: &Option<ItemStack>| {
                slot.map(|carried| (carried.item.index(), carried.count.get()))
            };
            inventory.slots().iter().map(carried).collect()
        }

        /// A hero of `slot` that walks a meter a tick.
        pub(crate) fn hero(
            world: &mut World,
            pos: impl Into<Position>,
            team: Team,
            slot: u32,
            combatant: Armed,
        ) -> StableId {
            let pos = pos.into();
            let combatant = combatant.bundle(world);
            let parts = (
                combatant,
                Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
                Owner::new(PlayerSlot::new(slot)),
            );
            Units::spawn(world, pos, team, parts)
        }
    }
}

#[cfg(test)]
mod tests;
