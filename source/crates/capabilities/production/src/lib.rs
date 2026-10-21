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
use campfire_actions::{ActionsSystems, ByAction};
use campfire_common::EnumAll;
use campfire_declarations::{
    ByType, ClientForm, Declarations, InstallCx, Module, ModuleSystems, ServerForm, SetPlace,
};
use campfire_players::Player;
use campfire_script::{ApiVersion, Status};
use campfire_sim::{Capability, SimSystems};
use campfire_stats::StatsSystems;
use campfire_units::{BodyUnit, StatusTags, UnitGrid, UnitTypes, UnitsSystems};
use campfire_values::{Block, EngineTag, TagProperties};
use campfire_vision::{ByHolding, ByVision, TeamSight};

use crate::holdings::Holdings;
use crate::production_column::RowReads;

mod book_builders;
mod build_book;
mod build_data;
mod build_start;
mod builder;
mod completed_sites;
mod construct_data;
mod construction;
mod drop_off_data;
mod error;
mod frame_point;
mod gather_data;
mod gather_loop;
mod gather_spec;
mod gatherer;
mod holdings;
mod mode_supply_data;
mod node;
mod node_book;
mod node_data;
mod order_dues;
mod placement;
mod placement_data;
mod player_supply;
mod producer_spec;
mod production_api;
mod production_books;
mod production_column;
mod production_data;
mod production_directive;
mod production_orders;
mod rally;
mod rally_target;
mod requirement_book;
mod requires_data;
mod resource_set;
mod site;
mod supply;
mod supply_book;
mod supply_cost_book;
mod supply_data;
mod train_data;
mod train_queue;
mod train_spec;
mod trained_units;
mod trains;

pub use crate::build_book::{BuildBook, BuildSpec, NewBuild, PlacementCheck, Style};
pub use crate::build_data::BuildData;
pub use crate::build_start::BuildStart;
pub use crate::builder::{BuildOrder, Builder};
pub use crate::completed_sites::{CompletedSites, OnSiteComplete};
pub use crate::construct_data::ConstructData;
pub use crate::construction::Construction;
pub use crate::construction::step::{StartingBuild, Step};
pub use crate::drop_off_data::DropOffData;
pub use crate::error::{PointPastBound, ProductionDataError, ProductionRefusal};
pub use crate::frame_point::FramePoint;
pub use crate::gather_data::GatherData;
pub use crate::gather_loop::GatherLoop;
pub use crate::gather_spec::GatherSpec;
pub use crate::gatherer::{GatherOrder, GatherStep, Gatherer, Load, NodeAt};
pub use crate::mode_supply_data::ModeSupplyData;
pub use crate::node::Node;
pub use crate::node_book::{NodeBook, NodeSpec};
pub use crate::node_data::{EmptyNode, NodeData};
pub use crate::placement::Placement;
pub use crate::placement_data::{PlacementData, PlacementRule};
pub use crate::player_supply::PlayerSupply;
pub use crate::producer_spec::ProducerSpec;
pub use crate::production_api::ProductionApi;
pub use crate::production_column::ProductionColumn;
pub use crate::production_data::ProductionData;
pub use crate::production_directive::ProductionDirective;
pub use crate::production_orders::ProductionOrders;
pub use crate::rally::Rally;
pub use crate::rally_target::RallyTarget;
pub use crate::requirement_book::{Required, RequirementBook};
pub use crate::requires_data::{RequiredUnit, RequiresData};
pub use crate::resource_set::ResourceSet;
pub use crate::site::{Progressed, Site, SiteProgress, SiteStart};
pub use crate::supply_book::SupplyBook;
pub use crate::supply_cost_book::{SupplyCostBook, SupplyUnit, UnitSupply};
pub use crate::supply_data::SupplyData;
pub use crate::train_data::TrainData;
pub use crate::train_queue::{QueueDues, Queued, TrainQueue};
pub use crate::train_spec::TrainSpec;
pub use crate::trained_units::{OnUnitTrained, Trained, TrainedUnits};
pub use crate::trains::Trains;

/// The `production` capability: units that train others, through a queue, within their player's
/// supply and requirements.
#[derive(Debug)]
pub struct Production;

/// The steps of `production`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum ProductionSystems {
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

impl ModuleSystems for ProductionSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        let inputs = || SetPlace::stage(SimSystems::Inputs);
        match self {
            Self::Commands => SetPlace::at(ActionsSystems::Queue),
            // After the actions' last step of Inputs too, as a match without stats has no grant
            // to order through.
            Self::CheckBuilds => inputs()
                .after(ActionsSystems::HoldAtInputs)
                .after(StatsSystems::GrantAtInputs),
            Self::Start => SetPlace::stage(SimSystems::Act).after(ActionsSystems::Start),
            Self::Finish => SetPlace::stage(SimSystems::Mode),
            Self::Supply => SetPlace::at(UnitsSystems::End),
        }
    }
}

impl Module for Production {
    const CAPABILITY: Option<Capability> = Some(Capability::Production);
    const NEEDS: &'static [Capability] = &[];
    const USES: &'static [Capability] = &[Capability::Stats, Capability::Vision];
    type Systems = ProductionSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<ProductionData>();
        d.observer(ProductionOrders::ordered);
        d.section::<SupplyData>();
        d.section::<NodeData>();
        d.section::<DropOffData>();
        d.section::<ModeSupplyData>();
        d.removals::<Gatherer>();
        d.engine_tags(&[
            Self::CONSTRUCTING,
            Self::NODE,
            Self::DROP_OFF,
            Self::GATHERING,
        ]);
        d.component::<TrainQueue>();
        d.timed::<TrainQueue>();
        d.observer(TrainQueue::revived);
        d.cache::<Holdings>();
        d.observer(Holdings::inserted);
        d.observer(Holdings::removed);
        d.observer(Holdings::aged);
        d.derived_sent::<PlayerSupply>();
        d.server_part::<Player, PlayerSupply>();
        d.component::<Rally>();
        d.component::<Builder>();
        d.timed::<Builder>();
        d.component::<Site>();
        d.component::<SiteProgress>();
        d.resource::<TrainedUnits>();
        d.resource::<CompletedSites>();
        d.hook::<OnUnitTrained>(Status::Runs(ApiVersion::FIRST));
        d.hook::<OnSiteComplete>(Status::Runs(ApiVersion::FIRST));
        d.component::<Node>();
        d.component::<Gatherer>();
        d.timed::<Gatherer>();
        d.extend::<UnitTypes>(Self::tag_types);
        d.book::<ByType<ProducerSpec>>(ProducerSpec::book);
        d.listed::<TrainData>();
        d.listed::<BuildData>();
        d.listed::<GatherData>();
        d.book::<ByAction<TrainSpec>>(TrainSpec::book);
        d.book::<ByAction<GatherSpec>>(GatherSpec::book);
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
            at.of_type::<ProducerSpec>().map(|_| TrainQueue::default())
        });
        d.part::<BodyUnit, Rally>(|at| at.of_type::<ProducerSpec>().map(|_| Rally::default()));
        d.part::<BodyUnit, Builder>(|at| at.of_type::<Builder>().copied());
        d.part::<BodyUnit, Node>(|at| at.of_type::<Node>().copied());
        d.part::<BodyUnit, Gatherer>(|at| at.of_type::<Gatherer>().copied());
        // A gatherer shows its loop in its status tags; a site gets its own at its spawn.
        d.part::<BodyUnit, StatusTags>(|at| {
            at.of_type::<Gatherer>().map(|_| StatusTags::default())
        });
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
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.by_role(
            ProductionSystems::Commands,
            ProductionOrders::apply_cancels::<ByVision>,
            ProductionOrders::apply_cancels::<ByHolding>,
        );
        cx.server(ProductionSystems::Commands, ProductionOrders::set_rallies);
        if cx.book::<SupplyBook>().is_some_and(|book| book.counts()) {
            cx.server(ProductionSystems::Supply, PlayerSupply::refresh);
        }
        // A build places a box, which only navigation tests for room, and a worker walks.
        if !cx.has(Capability::Navigation) {
            cx.by_role(
                ProductionSystems::Start,
                Trains::start_trains::<ServerForm>,
                Trains::start_trains::<ClientForm>,
            );
            cx.server(ProductionSystems::Finish, Trains::finish_trains);
            return;
        }
        cx.by_role(
            ProductionSystems::CheckBuilds,
            Self::check_builds::<ByVision>(),
            Self::check_builds::<ByHolding>(),
        );
        cx.by_role(
            ProductionSystems::Start,
            Self::start_builds::<ByVision>(),
            Self::start_builds::<ByHolding>(),
        );
        cx.by_role(
            ProductionSystems::Finish,
            Construction::progress_sites::<ServerForm>,
            Construction::progress_sites::<ClientForm>,
        );
        cx.server(
            ProductionSystems::Finish,
            Trains::finish_trains.after(Construction::progress_sites::<ServerForm>),
        );
        cx.server(ProductionSystems::Supply, GatherLoop::despawn_empty_nodes);
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
        UnitGrid::before_reading(
            (Construction::check_builds::<S>, GatherLoop::check_gathers).chain(),
        )
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
        )
            .chain()
            .into_configs()
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_actions::internals::TestAction;
    use campfire_actions::{
        ActionData, ActionKind, ActionRange, ActionTarget, Actions, Aim, ByAction, ProgressDues,
    };
    use campfire_common::Ticks;
    use campfire_declarations::ByTypeBuilder;
    use campfire_declarations::internals::BookLoad;
    use campfire_math::Num;
    use campfire_players::ResourceAmount;
    use campfire_script::{Filter, MatchNames};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{IdAllocator, StableId};
    use campfire_units::{BuildTarget, Life};
    use campfire_values::{ActionId, PackageIndex, TypeScope, UnitType};

    use crate::Production;
    use crate::build_book::{BuildBook, NewBuild};
    use crate::builder::Builder;
    use crate::gather_spec::GatherSpec;
    use crate::gatherer::{GatherStep, Gatherer};
    use crate::mode_supply_data::ModeSupplyData;
    use crate::site::SiteProgress;
    use crate::supply_book::SupplyBook;
    use crate::supply_cost_book::SupplyCostBook;
    use crate::supply_data::SupplyData;
    use crate::train_data::TrainData;
    use crate::train_spec::TrainSpec;

    /// A build tests add: its range in meters, its time, its cost in a player resource, and what
    /// it places, as its build book holds it.
    #[derive(Debug, Clone)]
    pub struct TestBuild<'a> {
        pub range: Num,
        pub time: Ticks,
        pub resource_cost: Option<ResourceAmount>,
        pub build: NewBuild<'a>,
    }

    impl Production {
        /// Adds a train of `unit_type` that takes `time` and costs `resource_cost`, no pool and no
        /// cooldown, to the match's action book and, in a match that runs production, its
        /// trains' book; in one that does not, it is an action of the train kind alone, which no
        /// pass trains.
        pub fn add_train(
            world: &mut World,
            unit_type: UnitType,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            let action = TestAction {
                resource_cost,
                ..TestAction::new(ActionKind::Train, Aim::None, ActionRange::Global, time)
            };
            let id = Actions::add(world, "train", action);
            if world.contains_resource::<ByAction<TrainSpec>>() {
                let spec = TrainSpec { unit_type };
                ByAction::<TrainSpec>::amend(world, |_, book| book.set(id, spec));
            }
            id
        }

        /// Resolves what the train `action`, which the action book of `world` loaded from `data`
        /// of `package`, makes, into the match's trains' book, as the load resolves it.
        pub fn load_train(
            world: &mut World,
            action: ActionId,
            data: &ActionData,
            package: PackageIndex,
        ) {
            let Some(train) = data.section::<TrainData>() else {
                return;
            };
            let scope = TypeScope::of_package(package);
            let names = world.resource::<MatchNames>();
            let unit_type = names.named((scope, train.unit_type.as_str()));
            let unit_type = unit_type.expect("a test's train makes a type of its package");
            ByAction::<TrainSpec>::amend(world, |_, book| {
                book.set(action, TrainSpec { unit_type });
            });
        }

        /// Adds `build`, aimed at a point, no pool and no cooldown, to the match's action book
        /// and its build book.
        pub fn add_build(world: &mut World, build: TestBuild<'_>) -> ActionId {
            let action = TestAction {
                resource_cost: build.resource_cost,
                ..TestAction::new(
                    ActionKind::Build,
                    Aim::Point { clamp: false },
                    ActionRange::meters(build.range),
                    build.time,
                )
            };
            let id = Actions::add(world, "build", action);
            BuildBook::amend(world, |_, book| book.push(id, build.build));
            id
        }

        /// Adds a gather of `spec` from the nodes `aim` selects, within `range` meters, a trip of
        /// `time`, to the match's action book and its gathers' book.
        pub fn add_gather(
            world: &mut World,
            spec: GatherSpec,
            aim: Filter,
            range: Num,
            time: Ticks,
        ) -> ActionId {
            let action = TestAction::new(
                ActionKind::Gather,
                Aim::Unit(aim),
                ActionRange::meters(range),
                time,
            );
            let id = Actions::add(world, "gather", action);
            ByAction::<GatherSpec>::amend(world, |_, book| book.set(id, spec));
            id
        }

        /// Counts supply, up to `max`, with each type's `supplies`, and the trains the book holds.
        pub fn count_supply(world: &mut World, max: u32, supplies: &[(UnitType, SupplyData)]) {
            let mut types = ByTypeBuilder::default();
            for &(unit_type, supply) in supplies {
                types.set(unit_type, supply);
            }
            let costs = SupplyCostBook::new(types.build(), world.resource::<ByAction<TrainSpec>>());
            costs.load_into(world);
            (SupplyBook(Some(ModeSupplyData { max }))).load_into(world);
        }

        /// The next stable id.
        pub fn allocate_id(world: &mut World) -> StableId {
            world.resource_mut::<IdAllocator>().allocate()
        }

        /// Orders `producer` to train what its slot 0 holds.
        pub fn order_train(world: &mut World, producer: StableId) {
            let entity = world.unit_entity(producer);
            ProgressDues::change(world, producer, entity, |progress, dues| {
                progress.order(0, ActionTarget::None, dues);
            })
            .expect("a producer has its progress");
        }

        /// The depot `id`'s life, and its progress while a site.
        pub fn site(world: &World, id: StableId) -> (Num, Option<Num>) {
            let life = world.part::<Life>(id).current();
            let progress = world
                .try_part::<SiteProgress>(id)
                .map(|progress| progress.get());
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
