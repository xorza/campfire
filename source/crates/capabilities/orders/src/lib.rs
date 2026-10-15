#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleConfigs, SystemSet};
use bevy_ecs::system::ScheduleSystem;
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_sim::Capability;
use campfire_units::BodyUnit;
use campfire_vision::{ByHolding, ByVision, TeamSight};

mod ai;
mod ai_data;
mod destinations;
mod error;
mod next_think;
mod order_inputs;
mod orders_action;
mod orders_api;
mod resetting;
mod thinking;
mod unit_order;

pub use crate::ai::Ai;
pub use crate::ai_data::AiData;
pub use crate::destinations::Destinations;
pub use crate::error::AiError;
pub use crate::next_think::NextThink;
pub use crate::order_inputs::{OrderInputs, PlayerOrders};
pub use crate::orders_action::OrdersAction;
pub use crate::orders_api::OrdersApi;
pub use crate::resetting::Resetting;
pub use crate::thinking::Thinking;
pub use crate::unit_order::{OrderedUnit, UnitOrder};

campfire_common::enum_all! {
    /// The steps of `orders`.
    #[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) enum OrdersSet {
        /// The tick's orders become each unit's current order.
        Orders,
        /// Units follow their paths and chase their targets, before attacks start.
        Chase,
        /// AI of the units due this tick issues orders.
        Think,
    }
    /// Every step.
    pub const ALL;
}

impl ModuleSet for OrdersSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Orders);
    const ALL: &'static [OrdersSet] = &OrdersSet::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            OrdersSet::Orders => Phase::Order,
            OrdersSet::Chase => Phase::Chase,
            OrdersSet::Think => Phase::Think,
        })
    }
}

/// The `orders` capability: units that take orders from a player, or from the AI script of their
/// type.
#[derive(Debug)]
pub struct Orders;

impl Module for Orders {
    const CAPABILITY: Option<Capability> = Some(Capability::Orders);
    const NEEDS: &'static [Capability] = &[Capability::Navigation, Capability::Stats];
    const USES: &'static [Capability] = &[
        Capability::Production,
        Capability::Vision,
        Capability::Interaction,
    ];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<OrdersSet>();
        d.component::<NextThink>();
        d.part::<BodyUnit, NextThink>(|at| at.of_type::<Ai>().map(|_| NextThink::FIRST));
        d.component::<Resetting>();
        d.state::<BodyUnit, Resetting>();
        d.scratch::<PlayerOrders>();
        d.book::<ByType<Ai>>();
        d.api(OrdersApi::register);
    }

    /// Adds orders to a match: in Inputs, after the orders of the capabilities before it, the
    /// tick's orders are checked and become current; in Think, the resets whose units arrived
    /// end, then the units due this tick think; in Act, before combat starts attacks, units walk
    /// their paths and chase their targets. It builds on the core `Units` installs, on navigation
    /// and on stats. Without the core's scripts, as on a client, no unit thinks. An order's
    /// targets are checked as the server's vision sees them, or, on a predicting client, as the
    /// units it holds.
    fn install(cx: &mut InstallCx<'_>) {
        cx.by_role(
            Orders::check_orders::<ByVision>(),
            Orders::check_orders::<ByHolding>(),
        );
        cx.systems(
            (Destinations::follow_paths, Destinations::chase)
                .chain()
                .in_set(OrdersSet::Chase),
        );
        cx.scripted(
            (Thinking::end_dead_resets, Thinking::think)
                .chain()
                .in_set(OrdersSet::Think),
        );
    }
}

impl Orders {
    /// The systems that check the tick's orders, each order's targets as `S` sees them, and
    /// apply them.
    fn check_orders<S: TeamSight>() -> ScheduleConfigs<ScheduleSystem> {
        (
            OrderInputs::check_player_orders::<S>,
            OrderInputs::apply_player_orders,
        )
            .chain()
            .in_set(OrdersSet::Orders)
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_actions::{ActionSlots, InProgress, SlotTimes};
    use campfire_common::{PlayerSlot, Tick};
    use campfire_declare::ByType;
    use campfire_script::{ScriptFailures, ScriptId};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{StableId, TickRate};
    use campfire_units::{ScriptBook, UnitTypeCore, Units};
    use campfire_values::{PackagePath, Scalar, UnitType};

    use crate::Orders;
    use crate::ai::Ai;
    use crate::ai_data::AiData;
    use crate::error::AiError;

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

    impl Orders {
        /// A unit type of `tags` and `params`, and with the AI script `ai` when it has one, thinking
        /// every 250 ms.
        pub fn ai_type(
            world: &mut World,
            tags: &[&str],
            params: &[(&str, Scalar)],
            ai: Option<&str>,
        ) -> UnitType {
            let data = UnitTypeCore::of(tags, params);
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
        pub fn think(world: &mut World, inputs: &[(PlayerSlot, &[u8])]) {
            world.step_with(inputs);
            let failures = world.non_send::<ScriptFailures>().calls();
            assert_eq!(failures, []);
        }

        /// When the attack it winds up strikes, if one winds up.
        pub fn strikes_at(world: &World, id: StableId) -> Option<Tick> {
            world
                .part::<ActionSlots>(id)
                .in_progress()
                .and_then(InProgress::resolves_at)
        }

        /// When its weapon may attack again.
        pub fn ready_at(world: &World, id: StableId) -> Tick {
            world.part::<SlotTimes>(id).slot(0).unwrap().ready_at
        }
    }
}
