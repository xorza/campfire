use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_sim::{Capability, SimSet};

use crate::actions::ActionsSet;
use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::navigation::NavigationSet;
use crate::production::build_specs::BuildSpecs;
use crate::production::builder::Builder;
use crate::production::construction::Construction;
use crate::production::gather_loop::GatherLoop;
use crate::production::gatherer::Gatherer;
use crate::production::node::Node;
use crate::production::node_book::NodeBook;
use crate::production::player_supply::PlayerSupply;
use crate::production::production_api::ProductionApi;
use crate::production::production_column::{ProductionColumn, RowParts};
use crate::production::production_data::ProductionData;
use crate::production::rally::Rally;
use crate::production::requirements::Requirements;
use crate::production::site::Site;
use crate::production::supply_costs::SupplyCosts;
use crate::production::train_queue::TrainQueue;
use crate::production::trains::Trains;
use crate::stats::StatsSet;
use crate::units::by_type::ByType;
use crate::vision::team_sight::{ByHolding, ByVision, TeamSight};

#[cfg(feature = "internals")]
pub(crate) mod bench;
pub(crate) mod build_specs;
pub(crate) mod build_target;
pub(crate) mod builder;
pub(crate) mod construction;
pub(crate) mod drop_off_data;
pub(crate) mod gather_loop;
pub(crate) mod gatherer;
pub(crate) mod held;
pub(crate) mod holdings;
pub(crate) mod node;
pub(crate) mod node_book;
pub(crate) mod node_data;
pub(crate) mod placement;
pub(crate) mod player_supply;
pub(crate) mod production_api;
pub(crate) mod production_books;
pub(crate) mod production_column;
pub(crate) mod production_data;
pub(crate) mod rally;
pub(crate) mod rally_target;
pub(crate) mod requirements;
pub(crate) mod resource_set;
pub(crate) mod site;
pub(crate) mod supply;
pub(crate) mod supply_costs;
pub(crate) mod supply_data;
pub(crate) mod supply_rules;
pub(crate) mod train_queue;
pub(crate) mod trains;

/// The `production` capability: units that train others, through a queue, within their player's
/// supply and requirements.
#[derive(Debug)]
pub struct Production;

/// The systems of `production`, for the mode to order its own against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProductionSet {
    /// In `SimSet::Inputs`: the build orders that changed are checked, after the orders apply.
    CheckBuilds,
    /// In `SimSet::Mode`: the trains whose time ended spawn.
    Finish,
}

impl Declares for Production {
    const MODULE: Option<Capability> = Some(Capability::Production);

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<TrainQueue>();
        d.derived_sent::<PlayerSupply>();
        d.component::<Rally>();
        d.component::<Builder>();
        d.component::<Site>();
        d.component::<Node>();
        d.component::<Gatherer>();
        d.book::<ByType<ProductionData>>();
        d.book::<Requirements>();
        d.book::<SupplyCosts>();
        d.book::<BuildSpecs>();
        d.book::<NodeBook>();
        d.column::<ProductionColumn, RowParts>(ProductionColumn::fill_row);
        d.api(ProductionApi::register);
    }
}

impl Production {
    /// Adds production to a match: in Act, after the other orders start, ordered trains pass their
    /// checks, pay, and join their unit's queue; in Mode, before the mode's hooks, the trains
    /// whose time ended spawn. With navigation, which tests a box for room and walks a worker,
    /// construction and gathering too: in Inputs, the build and gather orders that changed are
    /// checked; in Act, after the trains, builds start and workers run their loops; in Mode,
    /// before the trains finish, sites grow; as the tick ends, the nodes that ran out despawn. A
    /// player's supply and a node's end are the server's: a predicting client derives neither,
    /// as it holds only the units its team sees. A build's placement tests the units the server's
    /// vision sees, or, on a predicting client, the units it holds.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        if cx.predicting() {
            Production::install_by::<ByHolding>(cx);
        } else {
            Production::install_by::<ByVision>(cx);
        }
    }

    /// `install`, each placement testing the units `S` sees.
    fn install_by<S: TeamSight>(cx: &mut InstallCx<'_>) {
        let navigation = cx.has(Capability::Navigation);
        let predicting = cx.predicting();
        let schedule = &mut *cx.schedule;
        schedule.add_systems((
            Trains::start_trains
                .in_set(SimSet::Act)
                .after(ActionsSet::Start),
            Trains::finish_trains
                .in_set(SimSet::Mode)
                .in_set(ProductionSet::Finish),
        ));
        if !predicting {
            schedule.add_systems(PlayerSupply::refresh.in_set(SimSet::Vision));
        }
        // A build places a box, which only navigation tests for room, and a worker walks.
        if navigation {
            schedule.add_systems((
                (Construction::check_builds::<S>, GatherLoop::check_gathers)
                    .chain()
                    .in_set(SimSet::Inputs)
                    .in_set(ProductionSet::CheckBuilds)
                    .after(StatsSet::Regenerate)
                    .after(NavigationSet::TrackStatics)
                    .after(ActionsSet::HoldAtInputs),
                (
                    Construction::forget_dead_builds,
                    Construction::start_builds::<S>,
                    GatherLoop::run,
                    GatherLoop::tag_gatherers,
                )
                    .chain()
                    .in_set(SimSet::Act)
                    .after(Trains::start_trains),
                Construction::progress_sites
                    .in_set(SimSet::Mode)
                    .before(ProductionSet::Finish),
            ));
            if !predicting {
                schedule.add_systems(GatherLoop::despawn_empty_nodes.in_set(SimSet::Vision));
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_common::PlayerSlot;
    use campfire_math::Num;
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{IdAllocator, Position, StableId};

    use crate::actions::action_book::ActionBook;
    use crate::actions::action_slots::ActionSlots;
    use crate::actions::action_target::ActionTarget;
    use crate::navigation::Navigation;
    use crate::orders::order::{Action, Order};
    use crate::production::Production;
    use crate::production::build_target::BuildTarget;
    use crate::production::builder::Builder;
    use crate::production::gatherer::{GatherStep, Gatherer};
    use crate::production::site::Site;
    use crate::production::supply_costs::SupplyCosts;
    use crate::production::supply_data::SupplyData;
    use crate::production::supply_rules::SupplyRules;
    use crate::stats::life::Life;
    use crate::units::Units;
    use crate::units::body::Body;
    use crate::units::by_type::ByType;
    use crate::units::move_step::MoveStep;
    use crate::units::owner::Owner;
    use crate::units::team::Team;
    use crate::units::team_set::TeamSet;
    use crate::units::unit_type::UnitType;
    use crate::vision::seen_by::SeenBy;

    impl Production {
        /// Counts supply, up to `max`, with each type's `supplies`, and the trains the book holds.
        pub(crate) fn count_supply(
            world: &mut World,
            max: u32,
            supplies: &[(UnitType, SupplyData)],
        ) {
            let mut types = ByType::default();
            for &(unit_type, supply) in supplies {
                types.set(unit_type, supply);
            }
            let costs = SupplyCosts::new(types, world.resource::<ActionBook>());
            world.insert_resource(costs);
            world.insert_resource(SupplyRules { max });
        }

        /// A unit of `unit_type` owned by `owner`, at the origin.
        pub(crate) fn owned(world: &mut World, unit_type: UnitType, owner: PlayerSlot) -> StableId {
            Units::spawn_of(
                world,
                unit_type,
                Position::ORIGIN,
                Team::new(1),
                Owner::new(owner),
            )
        }

        /// The next stable id.
        pub(crate) fn allocate_id(world: &mut World) -> StableId {
            world.resource_mut::<IdAllocator>().allocate()
        }

        /// Orders `producer` to train what its slot 0 holds.
        pub(crate) fn order_train(world: &mut World, producer: StableId) {
            let mut slots = world.part_mut::<ActionSlots>(producer);
            slots.order(0, ActionTarget::None);
        }

        /// Runs a tick in which player `slot` orders `action` to `id`.
        pub(crate) fn tick_ordering(world: &mut World, slot: u32, id: StableId, action: Action) {
            Production::give_orders(world, slot, &[(id, action)]);
        }

        /// Runs a tick in which player `slot` gives `orders`, each an action to a unit, in order.
        pub(crate) fn give_orders(world: &mut World, slot: u32, orders: &[(StableId, Action)]) {
            let orders: Vec<Order> = orders
                .iter()
                .map(|&(unit, action)| Order::one(unit, action))
                .collect();
            let payload = Order::payload(&orders);
            world.step_with(&[(PlayerSlot::new(slot), &payload)]);
        }

        /// A walker of `team` at `pos`, of a half meter, seen by `seen`, by all when `None`.
        pub(crate) fn yard_walker(
            world: &mut World,
            pos: impl Into<Position>,
            team: Team,
            seen: Option<TeamSet>,
        ) -> StableId {
            let pos = pos.into();
            let parts = (
                Body::new(Num::HALF).unwrap(),
                Navigation::walker(MoveStep::new(Num::ONE).unwrap()),
            );
            let id = Units::spawn(world, pos, team, parts);
            if let Some(teams) = seen {
                world.insert_parts(id, SeenBy::new(teams));
            }
            id
        }

        /// Runs a tick in which player 0 orders `builder` to build its depot at (`x`, `z`).
        pub(crate) fn build_at(world: &mut World, builder: StableId, x: i64, z: i64) {
            let target = BuildTarget::Point {
                x: Num::int(x),
                z: Num::int(z),
                angle: Num::ZERO,
            };
            Production::tick_ordering(world, 0, builder, Action::Build { slot: 0, target });
        }

        /// The depot `id`'s life, and its progress while a site.
        pub(crate) fn site(world: &World, id: StableId) -> (Num, Option<Num>) {
            let life = world.part::<Life>(id).current();
            let progress = world.try_part::<Site>(id).map(Site::progress);
            (life, progress)
        }

        /// Where `builder`'s build order builds.
        pub(crate) fn order_of(world: &World, builder: StableId) -> Option<BuildTarget> {
            world
                .part::<Builder>(builder)
                .order()
                .map(|order| order.target)
        }

        /// Runs a tick in which player 0 orders each of `workers` to gather at `node`.
        pub(crate) fn gather_at(world: &mut World, workers: &[StableId], node: StableId) {
            let target = ActionTarget::Unit(node);
            let orders: Vec<(StableId, Action)> = workers
                .iter()
                .map(|&unit| (unit, Action::Slot { slot: 0, target }))
                .collect();
            Production::give_orders(world, 0, &orders);
        }

        /// The step of `worker`'s gather loop, none when it gathers nothing.
        pub(crate) fn gather_step(world: &World, worker: StableId) -> Option<GatherStep> {
            let gatherer = world.part::<Gatherer>(worker);
            gatherer.order().map(|order| order.step)
        }

        /// The node `worker`'s loop is at.
        pub(crate) fn node_of(world: &World, worker: StableId) -> Option<StableId> {
            let gatherer = world.part::<Gatherer>(worker);
            gatherer.order().map(|order| order.node.node)
        }
    }
}

#[cfg(test)]
mod tests;
