// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (09-determinism-core.md, One step: an `i128`, rounded once).
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
use campfire_actions::ActionsSet;
use campfire_common::EnumAll;
use campfire_declare::{
    ByType, ClientForm, Declarations, InstallCx, Module, ModuleSet, ServerForm, SetPlace,
};
use campfire_sim::{Capability, SimSet};
use campfire_stats::StatsSet;
use campfire_units::{BodyUnit, Dead, Owner, Player, UnitTypes, UnitsSet};
use campfire_values::{Block, EngineTag, TagProperties, UnitType};
use campfire_vision::{ByHolding, ByVision, TeamSight};

use crate::holdings::Holdings;
use crate::production_column::RowReads;

mod book_builders;
mod build_book;
mod build_target;
mod builder;
mod construction;
mod drop_off_data;
mod error;
mod frame_point;
mod gather_loop;
mod gatherer;
mod holdings;
mod mode_supply_data;
mod node;
mod node_book;
mod node_data;
mod placement;
mod player_supply;
mod production_action;
mod production_api;
mod production_books;
mod production_column;
mod production_data;
mod production_orders;
mod rally;
mod rally_target;
mod requirement_book;
mod resource_set;
mod site;
mod supply;
mod supply_book;
mod supply_cost_book;
mod supply_data;
mod train_queue;
mod trains;

pub use crate::build_book::{BuildBook, BuildSpec, NewBuild, PlacementCheck, Style};
pub use crate::build_target::BuildTarget;
pub use crate::builder::{BuildOrder, Builder};
pub use crate::construction::Construction;
pub use crate::construction::step::{Start, Step};
pub use crate::drop_off_data::DropOffData;
pub use crate::error::{PointPastBound, ProductionDataError, ProductionRefusal};
pub use crate::frame_point::FramePoint;
pub use crate::gather_loop::GatherLoop;
pub use crate::gatherer::{GatherOrder, GatherStep, Gatherer, Load, NodeAt};
pub use crate::mode_supply_data::ModeSupplyData;
pub use crate::node::Node;
pub use crate::node_book::NodeBook;
pub use crate::node_data::NodeData;
pub use crate::placement::Placement;
pub use crate::player_supply::PlayerSupply;
pub use crate::production_action::ProductionAction;
pub use crate::production_api::ProductionApi;
pub use crate::production_column::ProductionColumn;
pub use crate::production_data::ProductionData;
pub use crate::production_orders::ProductionOrders;
pub use crate::rally::Rally;
pub use crate::rally_target::RallyTarget;
pub use crate::requirement_book::{Required, RequirementBook};
pub use crate::resource_set::ResourceSet;
pub use crate::site::{Progressed, Site};
pub use crate::supply_book::SupplyBook;
pub use crate::supply_cost_book::{SupplyCostBook, SupplyUnit, UnitSupply};
pub use crate::supply_data::SupplyData;
pub use crate::train_queue::{Queued, TrainQueue};
pub use crate::trains::Trains;

/// The `production` capability: units that train others, through a queue, within their player's
/// supply and requirements.
#[derive(Debug)]
pub struct Production;

/// The steps of `production`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ProductionSet {
    /// The tick's orders cancel trains and builds, and set rally points.
    Commands,
    /// The build orders that changed are checked, after the orders apply.
    CheckBuilds,
    /// After the actions start: trains start, builds start and workers gather.
    Start,
    /// Sites progress, and the trains whose time ended spawn.
    Finish,
    /// Each player's supply is counted again, and the emptied nodes go.
    Supply,
}

impl ModuleSet for ProductionSet {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        let inputs = || SetPlace::stage(SimSet::Inputs);
        match self {
            Self::Commands => SetPlace::at(ActionsSet::Queue),
            Self::CheckBuilds => inputs().after(StatsSet::GrantAtInputs),
            Self::Start => SetPlace::stage(SimSet::Act).after(ActionsSet::Start),
            Self::Finish => SetPlace::stage(SimSet::Mode),
            Self::Supply => SetPlace::at(UnitsSet::End),
        }
    }
}

impl Module for Production {
    const CAPABILITY: Option<Capability> = Some(Capability::Production);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[
        Capability::Navigation,
        Capability::Stats,
        Capability::Vision,
    ];
    type Set = ProductionSet;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ProductionData>();
        d.section::<SupplyData>();
        d.section::<NodeData>();
        d.section::<DropOffData>();
        d.section::<ModeSupplyData>();
        d.removals::<Dead>();
        d.removals::<Gatherer>();
        d.removals::<Owner>();
        d.removals::<Site>();
        d.removals::<TrainQueue>();
        d.removals::<UnitType>();
        d.engine_tags(&[
            Self::CONSTRUCTING,
            Self::NODE,
            Self::DROP_OFF,
            Self::GATHERING,
        ]);
        d.component::<TrainQueue>();
        d.scratch::<Holdings>();
        d.derived_sent::<PlayerSupply>();
        d.server_part::<Player, PlayerSupply>();
        d.component::<Rally>();
        d.component::<Builder>();
        d.component::<Site>();
        d.component::<Node>();
        d.component::<Gatherer>();
        d.extend::<UnitTypes>(Self::tag_types);
        d.book::<ByType<ProductionData>>(ProductionData::book);
        d.book::<RequirementBook>(RequirementBook::build);
        d.book::<SupplyCostBook>(SupplyCostBook::build);
        d.book::<BuildBook>(BuildBook::build);
        d.book::<NodeBook>(NodeBook::build);
        d.book::<SupplyBook>(SupplyBook::build);
        d.rule(Self::rule);
        d.rule(Self::actions_rule);
        d.column::<ProductionColumn, RowReads>(ProductionColumn::fill_row);
        d.api(ProductionApi::register);
        d.book::<ByType<Builder>>(Builder::book);
        d.book::<ByType<Node>>(Node::book);
        d.book::<ByType<Gatherer>>(Gatherer::book);
        d.part::<BodyUnit, TrainQueue>(|at| {
            at.of_type::<ProductionData>()
                .map(|_| TrainQueue::default())
        });
        d.part::<BodyUnit, Rally>(|at| at.of_type::<ProductionData>().map(|_| Rally::default()));
        d.part::<BodyUnit, Builder>(|at| at.of_type::<Builder>().copied());
        d.part::<BodyUnit, Node>(|at| at.of_type::<Node>().copied());
        d.part::<BodyUnit, Gatherer>(|at| at.of_type::<Gatherer>().copied());
    }

    /// Adds production to a match: in Inputs, the tick's orders cancel trains and builds, and, on
    /// the server, set rally points to points or the units its vision sees; in Act, after the other
    /// orders start, ordered trains pass their checks, pay, and join their unit's queue; in Mode,
    /// before the mode's hooks, the trains whose time ended spawn. With navigation, which tests a
    /// box for room and walks a worker, construction and gathering too: in Inputs, the build and
    /// gather orders that changed are checked; in Act, after the trains, builds start and workers
    /// run their loops; in Mode, before the trains finish, sites grow; as the tick ends, the nodes
    /// that ran out despawn. A player's supply, a node's end, and the units trains and builds spawn
    /// are the server's: a predicting client derives none, as it holds only the units its team
    /// sees, and its unit comes back by replication. A build's placement tests the units the
    /// server's vision sees, or, on a predicting client, the units it holds. On the server, a
    /// player spawns with its supply, which the refresh of a mode that counts supply writes in
    /// place; a client receives its own player's.
    fn install(cx: &mut InstallCx<'_, Self::Set>) {
        cx.by_role(
            ProductionSet::Commands,
            ProductionOrders::apply_cancels::<ByVision>,
            ProductionOrders::apply_cancels::<ByHolding>,
        );
        cx.server(ProductionSet::Commands, ProductionOrders::set_rallies);
        if cx.book::<SupplyBook>().is_some_and(|book| book.counts()) {
            cx.server(ProductionSet::Supply, PlayerSupply::refresh);
        }
        // A build places a box, which only navigation tests for room, and a worker walks.
        if !cx.has(Capability::Navigation) {
            cx.by_role(
                ProductionSet::Start,
                (Trains::start_trains::<ServerForm>, Holdings::forget_read).chain(),
                (Trains::start_trains::<ClientForm>, Holdings::forget_read).chain(),
            );
            cx.server(ProductionSet::Finish, Trains::finish_trains);
            return;
        }
        cx.by_role(
            ProductionSet::CheckBuilds,
            Self::check_builds::<ByVision>(),
            Self::check_builds::<ByHolding>(),
        );
        cx.by_role(
            ProductionSet::Start,
            Self::start_builds::<ByVision>(),
            Self::start_builds::<ByHolding>(),
        );
        cx.add(ProductionSet::Finish, Construction::progress_sites);
        cx.server(
            ProductionSet::Finish,
            Trains::finish_trains.after(Construction::progress_sites),
        );
        cx.server(ProductionSet::Supply, GatherLoop::despawn_empty_nodes);
    }
}

impl Production {
    /// The engine tag of a site, a building under construction: it blocks its attacks, casts and
    /// uses, so no action of its starts.
    pub const CONSTRUCTING: EngineTag = EngineTag::new(
        4,
        "constructing",
        TagProperties::NONE
            .with_block(Block::Attack)
            .with_block(Block::Cast)
            .with_block(Block::Use)
            .with_block(Block::Produce),
    );
    /// The engine tag of node types, which workers gather from.
    pub const NODE: EngineTag = EngineTag::new(5, "node", TagProperties::NONE);
    /// The engine tag of drop-off types, which workers take their loads to.
    pub const DROP_OFF: EngineTag = EngineTag::new(6, "drop_off", TagProperties::NONE);
    /// The engine tag of a worker in its gather loop, which another such worker passes through.
    pub const GATHERING: EngineTag =
        EngineTag::new(7, "gathering", TagProperties::NONE.with_passes());

    /// The checks of build and gather orders, each placement testing the units `S` sees.
    fn check_builds<S: TeamSight>() -> ScheduleConfigs<ScheduleSystem> {
        (Construction::check_builds::<S>, GatherLoop::check_gathers)
            .chain()
            .into_configs()
    }

    /// The starts of the trains, the builds and the gathers, for the sight of `S`.
    fn start_builds<S: TeamSight>() -> ScheduleConfigs<ScheduleSystem> {
        (
            Trains::start_trains::<S>,
            Construction::forget_dead_builds,
            Construction::start_builds::<S>,
            GatherLoop::forget_dead_gathers,
            GatherLoop::run::<S>,
            GatherLoop::tag_gatherers,
            Holdings::forget_read,
        )
            .chain()
            .into_configs()
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_actions::{ActionBook, ActionProgress, ActionTarget};
    use campfire_declare::{Book, ByType};
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{IdAllocator, StableId};
    use campfire_stats::Life;
    use campfire_values::UnitType;

    use crate::Production;
    use crate::build_target::BuildTarget;
    use crate::builder::Builder;
    use crate::gatherer::{GatherStep, Gatherer};
    use crate::mode_supply_data::ModeSupplyData;
    use crate::site::Site;
    use crate::supply_book::SupplyBook;
    use crate::supply_cost_book::SupplyCostBook;
    use crate::supply_data::SupplyData;

    impl Production {
        /// Counts supply, up to `max`, with each type's `supplies`, and the trains the book holds.
        pub fn count_supply(world: &mut World, max: u32, supplies: &[(UnitType, SupplyData)]) {
            let mut types = ByType::default();
            for &(unit_type, supply) in supplies {
                types.set(unit_type, supply);
            }
            let costs = SupplyCostBook::new(types, world.resource::<ActionBook>());
            costs.load_into(world);
            (SupplyBook(Some(ModeSupplyData { max }))).load_into(world);
        }

        /// The next stable id.
        pub fn allocate_id(world: &mut World) -> StableId {
            world.resource_mut::<IdAllocator>().allocate()
        }

        /// Orders `producer` to train what its slot 0 holds.
        pub fn order_train(world: &mut World, producer: StableId) {
            let mut progress = world.part_mut::<ActionProgress>(producer);
            progress.order(0, ActionTarget::None);
        }

        /// The depot `id`'s life, and its progress while a site.
        pub fn site(world: &World, id: StableId) -> (Num, Option<Num>) {
            let life = world.part::<Life>(id).current();
            let progress = world.try_part::<Site>(id).map(Site::progress);
            (life, progress)
        }

        /// Where `builder`'s build order builds.
        pub fn order_of(world: &World, builder: StableId) -> Option<BuildTarget> {
            world
                .part::<Builder>(builder)
                .order()
                .map(|order| order.target)
        }

        /// The step of `worker`'s gather loop, none when it gathers nothing.
        pub fn gather_step(world: &World, worker: StableId) -> Option<GatherStep> {
            let gatherer = world.part::<Gatherer>(worker);
            gatherer.order().map(|order| order.step)
        }

        /// The node `worker`'s loop is at.
        pub fn node_of(world: &World, worker: StableId) -> Option<StableId> {
            let gatherer = world.part::<Gatherer>(worker);
            gatherer.order().map(|order| order.node.node)
        }
    }
}
