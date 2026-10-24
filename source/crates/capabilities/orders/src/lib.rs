// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
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
use campfire_actions::{ActionSlots, ActionsSystems};
use campfire_common::EnumAll;
use campfire_declarations::{
    ById, Declarations, InstallCx, Module, ModuleSystems, NamedScripts, SetPlace,
};
use campfire_navigation::NavigationSystems;
use campfire_script::{ApiVersion, Status};
use campfire_sim::{Capability, SimSystems};
use campfire_stats::ModifierAppliers;
use campfire_units::{BodyUnit, Due, UnitGrid};
use campfire_values::UnitType;
use campfire_vision::{ByHolding, ByVision, TeamSight};

use crate::ai::OnThink;
use crate::thinking::ThinkDue;

mod acquire;
mod acquire_data;
mod acquiring;
mod ai;
mod ai_data;
mod attack_move;
mod destinations;
mod dropped_orders;
mod error;
mod next_turn;
mod orders_api;
mod orders_directive;
mod orders_effect;
mod resetting;
mod thinking;
mod unit_order;
mod unit_orders;

pub use crate::acquire::{Acquire, NextScan};
pub use crate::acquire_data::{AcquireData, AcquirePatch};
pub use crate::acquiring::Acquiring;
pub use crate::ai::{Ai, NextThink};
pub use crate::ai_data::AiData;
pub use crate::attack_move::AttackMove;
pub use crate::destinations::Destinations;
pub use crate::dropped_orders::{DroppedOrder, DroppedOrders, OrderDropCause};
pub use crate::error::{AcquireError, AiError, OrdersRefusal};
pub use crate::next_turn::{NextTurn, TurnSpec};
pub use crate::orders_api::OrdersApi;
pub use crate::orders_directive::OrdersDirective;
pub use crate::resetting::Resetting;
pub use crate::thinking::Thinking;
pub use crate::unit_order::{HeldOrder, UnitOrder};
pub use crate::unit_orders::{OrderQueue, UnitOrders};

/// The steps of `orders`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum OrdersSystems {
    /// The tick's commands become each unit's current order.
    Commands,
    /// Units chase their targets, after path walkers walk, before attacks start.
    Chase,
    /// The idle units due this tick scan for a target, before AI thinks.
    Scan,
    /// AI of the units due this tick issues orders.
    Think,
}

impl ModuleSystems for OrdersSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Commands => SetPlace::at(ActionsSystems::Order),
            Self::Chase => {
                SetPlace::at(ActionsSystems::Chase).after(NavigationSystems::FollowPaths)
            }
            Self::Scan => SetPlace::stage(SimSystems::Think).before(Self::Think),
            Self::Think => SetPlace::stage(SimSystems::Think),
        }
    }
}

/// The `orders` capability: units that take orders from a player, or from the AI script of their
/// type.
#[derive(Debug)]
pub struct Orders;

impl Module for Orders {
    const CAPABILITY: Option<Capability> = Some(Capability::Orders);
    const NEEDS: &'static [Capability] = &[Capability::Navigation, Capability::Stats];
    const USES: &'static [Capability] = &[Capability::Vision];
    type Systems = OrdersSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<AiData>();
        d.section::<AcquireData>();
        d.component::<NextScan>();
        d.timed::<NextScan>();
        d.observer(NextScan::revived);
        d.part::<BodyUnit, NextScan>(|at| at.of_type::<Acquire>().map(|_| NextScan::FIRST));
        d.component::<AttackMove>();
        d.timed::<AttackMove>();
        d.part::<BodyUnit, AttackMove>(|at| at.of_type::<Acquire>().map(|_| AttackMove::NONE));
        d.book::<ById<UnitType, Acquire>>(Acquire::book);
        d.component::<NextThink>();
        d.timed::<NextThink>();
        d.observer(NextThink::revived);
        d.part::<BodyUnit, NextThink>(|at| at.of_type::<Ai>().map(|_| NextThink::FIRST));
        d.component::<HeldOrder>();
        d.require::<ActionSlots, HeldOrder>();
        d.part_with::<ActionSlots, HeldOrder>();
        d.component::<Resetting>();
        d.state::<BodyUnit, Resetting>();
        d.scratch::<OrderQueue>();
        d.report::<DroppedOrders>();
        d.scratch::<Due<ThinkDue>>();
        d.book::<ById<UnitType, Ai>>(Ai::book);
        d.rule(Ai::avatar_rule);
        d.extend::<ModifierAppliers>(Ai::applied_ways);
        d.extend::<NamedScripts>(Ai::named_scripts);
        d.api(OrdersApi::register);
        d.hook::<OnThink>(Status::Runs(ApiVersion::FIRST));
    }

    /// Adds orders to a match: in Inputs, after the orders of the capabilities before it, the
    /// tick's orders are checked and become current; in Think, on a match that computes the state,
    /// the units that attack-move and stand walk on and the idle units due scan for a target, then
    /// the resets whose units arrived end, and the
    /// units due this tick think; in Act, before combat starts attacks, after path walkers
    /// walk, units chase their targets. It builds on the core `Units` installs, on navigation and
    /// on stats. Without the core's scripts, as on a client, no unit thinks. An order's targets are
    /// checked as the server's vision sees them, or, on a predicting client, as the units it holds.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.by_role(
            OrdersSystems::Commands,
            Self::check_orders::<ByVision>(),
            Self::check_orders::<ByHolding>(),
        );
        cx.add(OrdersSystems::Chase, Destinations::chase);
        cx.server(
            OrdersSystems::Scan,
            (
                AttackMove::walk_on,
                UnitGrid::before_reading(Acquiring::scan),
            )
                .chain(),
        );
        cx.scripted(
            OrdersSystems::Think,
            (
                Thinking::end_dead_resets,
                Thinking::collect_thinkers,
                Thinking::think,
            )
                .chain(),
        );
    }
}

impl Orders {
    /// The systems that check the tick's orders, each order's targets as `S` sees them, and
    /// apply them.
    fn check_orders<S: TeamSight>() -> ScheduleConfigs<ScheduleSystem> {
        (
            UnitOrders::release_held,
            UnitOrders::check_player_orders::<S>,
            UnitOrders::apply_player_orders,
        )
            .chain()
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_actions::{ActionProgress, InProgress, SlotTimes};
    use campfire_common::{PlayerSlot, Tick};
    use campfire_declarations::ById;
    use campfire_declarations::internals::BookLoad;
    use campfire_script::{Filters, MatchNames, ScriptFailures, ScriptId};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{MatchRate, StableId};
    use campfire_units::{ScriptBook, UnitTypeCore, Units};
    use campfire_values::{FilterData, PackagePath, Scalar, UnitType};

    use crate::Orders;
    use crate::acquire::Acquire;
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
            let rate = *world.resource::<MatchRate>();
            let ai = Ai::of(data, script, world.resource::<ScriptBook>(), rate)?;
            ById::<UnitType, Ai>::amend(world, |_, book| book.set(unit_type, ai));
            Ok(())
        }
    }

    impl Orders {
        /// Gives `unit_type` its idle scan, as its `acquire` section would: each `scan_ms`, at
        /// the match's rate, of the units any of `targets` selects.
        pub fn load_acquire(
            world: &mut World,
            unit_type: UnitType,
            scan_ms: u64,
            targets: &[&str],
        ) {
            let rate = *world.resource::<MatchRate>();
            let targets: Vec<FilterData> = targets
                .iter()
                .map(|text| FilterData::parse(text).expect("a test's filter"))
                .collect();
            let tags = world.resource::<MatchNames>().tags();
            let acquire = Acquire {
                period: rate.duration(scan_ms).expect("a test's period"),
                targets: Filters::resolve(&targets, tags).expect("a test's tags"),
            };
            ById::<UnitType, Acquire>::amend(world, |_, book| book.set(unit_type, acquire));
        }
    }

    impl Orders {
        /// A unit type of `tags` and `params`, and with the AI script `ai` when it has one,
        /// thinking every 250 ms.
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
                Self::load_ai(world, unit_type, &ai, script).unwrap();
            }
            unit_type
        }

        /// Runs a tick, and checks that no script call failed in it.
        pub fn think(world: &mut World, inputs: &[(PlayerSlot, &[u8])]) {
            world.step_with(inputs);
            let failures = world.resource_mut::<ScriptFailures>().take_calls();
            assert_eq!(failures, []);
        }

        /// When the attack it winds up strikes, if one winds up.
        pub fn strikes_at(world: &World, id: StableId) -> Option<Tick> {
            world
                .part::<ActionProgress>(id)
                .in_progress()
                .and_then(InProgress::resolves_at)
        }

        /// When its weapon may attack again.
        pub fn ready_at(world: &World, id: StableId) -> Tick {
            world.part::<SlotTimes>(id).slot(0).unwrap().ready_at
        }
    }
}
