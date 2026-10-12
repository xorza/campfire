//! Design 14's D12: a predicting client's schedule reads, on a unit it holds as the server sent
//! it, only what it derives itself or what reaches every client that holds the unit.

use std::collections::BTreeMap;

use bevy_ecs::component::{ComponentId, Immutable, Mutable};
use bevy_ecs::query::{ComponentAccessKind, FilteredAccess, FilteredAccessSet};
use bevy_ecs::schedule::Schedules;
use bevy_ecs::system::System;
use bevy_ecs::world::Mut;
use campfire_common::SegmentSeed;
use campfire_sim::{SimResource, SimUpdate};

use super::*;
use crate::state_types::Declared;
use crate::state_types::data_kind::DataKind;
use crate::state_types::recipients::Recipients;
use crate::state_types::replication::Replication;
use crate::stats::modifiers::Modifiers;
use crate::stats::unit_stats::UnitStats;

/// How a client comes to hold a state type: it derives it, or receives it as its kind's.
#[derive(Debug, Clone, Copy)]
enum Held {
    Derived,
    Of(DataKind),
}

/// Each state type of a match's lists by its id in `world`, with its name and how a client holds
/// it.
#[derive(Debug)]
struct Classes<'w> {
    world: &'w mut World,
    types: BTreeMap<ComponentId, (&'static str, Held)>,
}

impl Classes<'_> {
    fn of<C: Component>(&mut self, held: Held) {
        let id = self.world.register_component::<C>();
        self.types.insert(id, (type_name::<C>(), held));
    }
}

impl StateTypes for Classes<'_> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn server<C: Replication>(&mut self, _: Declared) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn derived<C: Component>(&mut self) {
        self.of::<C>(Held::Derived);
    }

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.of::<C>(Held::Of(C::KIND));
    }

    fn resource<R: SimResource>(&mut self) {}

    fn sim_component<C: Replication>(&mut self) {
        self.of::<C>(Held::Of(C::KIND));
    }
}

/// What `access` reads, a component's data or its presence, `Has` and `With` among them, and
/// whether only on the units a client predicts, as the default filters make a query that names
/// no `Unpredicted`. `None` for an access past what a list can name, as a system that holds the
/// world.
fn reads(access: &FilteredAccess, unpredicted: ComponentId) -> Option<(Vec<ComponentId>, bool)> {
    let sets = access.filter_sets();
    let predicted = !sets.is_empty() && sets.iter().all(|set| set.without().contains(unpredicted));
    let kinds = access.access().try_iter_access().ok()?;
    let mut reads: Vec<ComponentId> = kinds
        .map(|kind| *ComponentAccessKind::index(&kind))
        .collect();
    reads.extend(sets.iter().flat_map(|set| set.with().iter()));
    Some((reads, predicted))
}

/// The reads that wait on the question of `PLAN_NET_QUESTIONS.md`, A6: what a predicting
/// client does with the state types the owner of the units it predicts does not hold, the server's
/// and those of another player's entity. The test fails on a read this list lacks, and on one it
/// names that is gone, so no new read joins them while the question stands.
const AWAITING: [&str; 39] = [
    "abilities::casts::Casts::predict_casts reads players::player_resources::PlayerResources (Of(Resources))",
    "abilities::casts::Casts::start_casts reads players::player_resources::PlayerResources (Of(Resources))",
    "areas::area_life::AreaLife::trigger reads areas::area_trigger::AreaTrigger (Of(Server))",
    "combat::attacks::Attacks::start_attacks reads players::player_resources::PlayerResources (Of(Resources))",
    "combat::attacks::Attacks::strike reads players::player_resources::PlayerResources (Of(Resources))",
    "combat::dying::Dying::despawn_dead reads combat::on_death::OnDeath (Of(Server))",
    "combat::dying::Dying::die reads combat::on_death::OnDeath (Of(Server))",
    "combat::dying::Dying::respawn reads combat::recent_attackers::RecentAttackers (Of(Server))",
    "navigation::routing::Routing::route_units reads navigation::path_walker::PathWalker (Of(Server))",
    "orders::destinations::Destinations::follow_paths reads navigation::on_path::OnPath (Of(Server))",
    "orders::destinations::Destinations::follow_paths reads navigation::path_walker::PathWalker (Of(Server))",
    "orders::order_inputs::OrderInputs::apply_player_orders reads navigation::path_walker::PathWalker (Of(Server))",
    "orders::order_inputs::OrderInputs::apply_player_orders reads production::builder::Builder (Of(Server))",
    "orders::order_inputs::OrderInputs::apply_player_orders reads production::gatherer::Gatherer (Of(Server))",
    "orders::order_inputs::OrderInputs::check_player_orders reads vision::seen_by::SeenBy (Of(Server))",
    "orders::production_orders::ProductionOrders::apply_production_orders reads players::player_resources::PlayerResources (Of(Resources))",
    "orders::production_orders::ProductionOrders::apply_production_orders reads production::site::Site (Of(Server))",
    "orders::production_orders::ProductionOrders::apply_production_orders reads vision::seen_by::SeenBy (Of(Server))",
    "production::construction::Construction::check_builds reads players::player_resources::PlayerResources (Of(Resources))",
    "production::construction::Construction::check_builds reads production::builder::Builder (Of(Server))",
    "production::construction::Construction::check_builds reads production::site::Site (Of(Server))",
    "production::construction::Construction::check_builds reads vision::seen_by::SeenBy (Of(Server))",
    "production::construction::Construction::forget_dead_builds reads production::builder::Builder (Of(Server))",
    "production::construction::Construction::progress_sites reads production::builder::Builder (Of(Server))",
    "production::construction::Construction::progress_sites reads production::site::Site (Of(Server))",
    "production::construction::Construction::progress_sites reads units::status_tags::StatusTags (Of(Server))",
    "production::gather_loop::GatherLoop::check_gathers reads players::player_resources::PlayerResources (Of(Resources))",
    "production::gather_loop::GatherLoop::check_gathers reads production::gatherer::Gatherer (Of(Server))",
    "production::gather_loop::GatherLoop::check_gathers reads production::node::Node (Of(Server))",
    "production::gather_loop::GatherLoop::despawn_empty_nodes reads production::node::Node (Of(Server))",
    "production::gather_loop::GatherLoop::tag_gatherers reads production::gatherer::Gatherer (Of(Server))",
    "production::gather_loop::GatherLoop::tag_gatherers reads units::status_tags::StatusTags (Of(Server))",
    "production::player_supply::PlayerSupply::refresh reads production::site::Site (Of(Server))",
    "production::trains::Trains::start_trains reads players::player_resources::PlayerResources (Of(Resources))",
    "production::trains::Trains::start_trains reads production::site::Site (Of(Server))",
    "projectiles::flying::Flying::fly reads projectiles::flight_state::FlightState (Of(Server))",
    "stats::refresh::Refresh::run reads units::status_tags::StatusTags (Of(Server))",
    "vision::seeing::Seeing::see reads vision::seen_by::SeenBy (Of(Server))",
    "vision::seeing::Seeing::see reads vision::sight::Sight (Of(Server))",
];

#[test]
fn prediction_reads_only_what_its_client_derives_or_every_client_that_holds_the_unit_receives() {
    let set = Release::capabilities();
    let mut world = World::new();
    let rate = Match::TEST_RATE;
    let spec = MatchSpec::new(SegmentSeed::new([0; 32]), rate).capabilities(set);
    Release::build(&mut world, spec.predicting().unbuilt());
    let unpredicted = world.register_component::<Unpredicted>();
    let mut classes = Classes {
        world: &mut world,
        types: BTreeMap::new(),
    };
    Release::state_types(set, &mut classes);
    let types = classes.types;
    // Each read of a type a client holds of no unit it does not predict, by system, and each
    // system whose reads no list can name.
    let mut failures = Vec::new();
    let mut unbounded = Vec::new();
    // Each type read on units a client does not predict, which shows the check reads what it
    // means to.
    let mut read_unpredicted = Vec::new();
    world.resource_scope(|world, mut schedules: Mut<'_, Schedules>| {
        let schedule = schedules.get_mut(SimUpdate).expect("the sim's schedule");
        let systems = &mut schedule.graph_mut().systems;
        let keys: Vec<_> = systems.iter().map(|(key, ..)| key).collect();
        for key in keys {
            let mut accesses = Vec::new();
            let system = systems.get_mut(key).unwrap();
            let name = system.name().to_string();
            accesses.push(system.initialize(world));
            for condition in systems.get_conditions_mut(key).into_iter().flatten() {
                accesses.push(condition.condition.initialize(world));
            }
            let filtered = accesses
                .iter()
                .flat_map(FilteredAccessSet::filtered_accesses);
            for filtered in filtered {
                let Some((reads, predicted)) = reads(filtered, unpredicted) else {
                    unbounded.push(name.clone());
                    continue;
                };
                for read in reads {
                    let Some(&(type_name, held)) = types.get(&read) else {
                        continue;
                    };
                    if !predicted {
                        read_unpredicted.push(type_name);
                    }
                    // The owner of a unit its client predicts holds every kind but the server's.
                    let reaches = match held {
                        Held::Derived => true,
                        Held::Of(kind) if predicted => kind.is_sent(),
                        Held::Of(kind) => kind.audience() == Recipients::ALL,
                    };
                    if !reaches {
                        let read = format!("{name} reads {type_name} ({held:?})");
                        failures.push(read.replace("campfire_capabilities::", ""));
                    }
                }
            }
        }
    });
    // Aura carriers a client does not predict hold their modifiers on the units it does, and a
    // param reads each source's stats (design 14, D11).
    for type_name in [type_name::<Modifiers>(), type_name::<UnitStats>()] {
        assert!(read_unpredicted.contains(&type_name), "{type_name}");
    }
    failures.sort();
    failures.dedup();
    assert_eq!(failures, AWAITING);
    unbounded.sort();
    unbounded.dedup();
    assert_eq!(unbounded, Vec::<String>::new());
}
