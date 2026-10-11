use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::world::World;
use campfire_sim::{SimSet, StateRegistry};

use crate::actions::ActionsSet;
use crate::navigation::NavigationSet;
use crate::navigation::body_index::BodyIndex;
use crate::production::build_specs::BuildSpecs;
use crate::production::builder::Builder;
use crate::production::construction::Construction;
use crate::production::gather_loop::GatherLoop;
use crate::production::gatherer::Gatherer;
use crate::production::node::Node;
use crate::production::node_book::NodeBook;
use crate::production::production_column::{ProductionColumn, RowParts};
use crate::production::production_data::ProductionData;
use crate::production::rally::Rally;
use crate::production::requirements::Requirements;
use crate::production::site::Site;
use crate::production::supply_costs::SupplyCosts;
use crate::production::train_queue::TrainQueue;
use crate::production::trains::Trains;
use crate::state_types::StateTypes;
use crate::stats::StatsSet;
use crate::units::by_type::ByType;
use crate::units::view::View;

#[cfg(feature = "bench")]
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
pub(crate) mod production_api;
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

impl Production {
    /// Lists the state types it adds (design 14, D9).
    pub(crate) fn state_types<T: StateTypes>(types: &mut T) {
        types.component::<TrainQueue>();
        types.component::<Rally>();
        types.component::<Builder>();
        types.component::<Site>();
        types.component::<Node>();
        types.component::<Gatherer>();
    }

    /// Adds production to a match: in Act, after the other orders start, ordered trains pass their
    /// checks, pay, and join their unit's queue; in Mode, before the mode's hooks, the trains
    /// whose time ended spawn. With navigation, which tests a box for room and walks a worker,
    /// construction and gathering too: in Inputs, the build and gather orders that changed are
    /// checked; in Act, after the trains, builds start and workers run their loops; in Mode,
    /// before the trains finish, sites grow; as the tick ends, the nodes that ran out despawn.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(ByType::<ProductionData>::default());
        world.insert_resource(Requirements::default());
        world.insert_resource(SupplyCosts::default());
        world.insert_resource(BuildSpecs::default());
        world.insert_resource(NodeBook::default());
        let view = world.non_send::<View>().clone();
        view.add_column(ProductionColumn::default());
        view.add_source::<RowParts, _>(world, ProductionColumn::fill_row);
        schedule.add_systems((
            Trains::start_trains
                .in_set(SimSet::Act)
                .after(ActionsSet::Start),
            Trains::finish_trains
                .in_set(SimSet::Mode)
                .in_set(ProductionSet::Finish),
        ));
        // A build places a box, which only navigation tests for room, and a worker walks.
        if world.contains_resource::<BodyIndex>() {
            schedule.add_systems((
                (Construction::check_builds, GatherLoop::check_gathers)
                    .chain()
                    .in_set(SimSet::Inputs)
                    .in_set(ProductionSet::CheckBuilds)
                    .after(StatsSet::Regenerate)
                    .after(NavigationSet::TrackStatics)
                    .after(ActionsSet::HoldAtInputs),
                (
                    Construction::forget_dead_builds,
                    Construction::start_builds,
                    GatherLoop::run,
                    GatherLoop::tag_gatherers,
                )
                    .chain()
                    .in_set(SimSet::Act)
                    .after(Trains::start_trains),
                Construction::progress_sites
                    .in_set(SimSet::Mode)
                    .before(ProductionSet::Finish),
                GatherLoop::despawn_empty_nodes.in_set(SimSet::Vision),
            ));
        }
        Self::state_types(registry);
    }
}

#[cfg(test)]
mod tests;
