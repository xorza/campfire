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
pub(crate) mod player_supply;
pub(crate) mod production_api;
pub(crate) mod production_books;
pub(crate) mod production_column;
pub(crate) mod production_data;
pub(crate) mod production_world;
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
    /// before the trains finish, sites grow; as the tick ends, the nodes that ran out despawn.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        let navigation = cx.has(Capability::Navigation);
        let schedule = &mut *cx.schedule;
        schedule.add_systems((
            Trains::start_trains
                .in_set(SimSet::Act)
                .after(ActionsSet::Start),
            Trains::finish_trains
                .in_set(SimSet::Mode)
                .in_set(ProductionSet::Finish),
            PlayerSupply::refresh.in_set(SimSet::Vision),
        ));
        // A build places a box, which only navigation tests for room, and a worker walks.
        if navigation {
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
    }
}

#[cfg(test)]
mod tests;
