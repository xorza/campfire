//! (10-replication-audiences.md, Prediction reads only what its client holds): a predicting
//! client's schedule reads, on a unit it holds as the server sent it, only what it derives itself
//! or what reaches every client that holds the unit, and builds no system that computes the
//! server's own state.

use bevy_ecs::component::ComponentId;
use bevy_ecs::query::{ComponentAccessKind, FilteredAccess, FilteredAccessSet};
use bevy_ecs::schedule::SystemSet;
use bevy_ecs::system::{System, SystemAccess};
use campfire_declarations::ServerOnlySystems;
use campfire_stats::{Modifiers, UnitStats};

use super::*;
use crate::fixtures::package_set::PackageSet;
use crate::release::client_holds::ClientHolds;
use crate::release::tests::feature_installs::{books_of, every_system};

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

/// The names of the systems of the sim's schedule in `world`, built unbuilt.
fn system_names(world: &mut World) -> Vec<String> {
    let mut schedules = world.resource_mut::<Schedules>();
    let schedule = schedules.get_mut(SimUpdate).expect("the sim's schedule");
    let systems = &schedule.graph_mut().systems;
    let mut names: Vec<String> = systems
        .iter()
        .map(|(_, system, _)| {
            system
                .name()
                .to_string()
                .replace("campfire_capabilities::", "")
        })
        .collect();
    names.sort();
    names
}

/// The names of the systems of the sim's schedule in `world`, built, that only a server builds:
/// those `InstallCx::server` adds.
fn server_only_systems(world: &World) -> Vec<String> {
    let schedule = world.resource::<Schedules>().get(SimUpdate);
    let schedule = schedule.expect("the sim's schedule");
    let own = schedule.graph().systems_in_set(ServerOnlySystems.intern());
    let own = own.expect("a server builds systems of its own");
    let systems = schedule.systems().expect("the schedule is built");
    let mut names: Vec<String> = systems
        .filter(|(key, _)| own.contains(key))
        .map(|(_, system)| system.name().to_string())
        .collect();
    names.sort();
    names
}

/// The exclusive systems a predicting client builds: Bevy gives an exclusive system no access,
/// so the check cannot read what each reads, and `Release::sync_held` checks them by what they
/// do. The test fails on one this list lacks, and on one it names that is gone, so no new one
/// joins them unchecked.
const EXCLUSIVE: [&str; 3] = [
    "campfire_deliveries::delivery_hooks::DeliveryHooks::end",
    "campfire_production::construction::Construction::start_builds<campfire_vision::team_sight::ByHolding>",
    "campfire_production::gather_loop::GatherLoop::run<campfire_vision::team_sight::ByHolding>",
];

#[test]
fn prediction_reads_only_what_its_client_derives_or_every_client_that_holds_the_unit_receives() {
    let set = Release::capabilities();
    let mut world = World::new();
    let rate = Match::TEST_RATE;
    let spec = |role| MatchSetup::new(SegmentSeed::ZERO, rate, role);
    let rules = MatchKind::Rules;
    Release::build(
        &mut world,
        spec(Role::Client).unbuilt(),
        books_of(&PackageSet {
            rate,
            ..every_system()
        }),
        rules,
    );
    let unpredicted = world.register_component::<Unpredicted>();
    let holds = ClientHolds::of(&mut world, set);
    // Each read of a type a client holds of no unit it does not predict, by system, each system
    // whose reads no list can name, and each exclusive system.
    let mut failures = Vec::new();
    let mut unbounded = Vec::new();
    let mut exclusive = Vec::new();
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
            let access = system.initialize(world);
            if access.is_exclusive() {
                exclusive.push(name.replace("campfire_capabilities::", ""));
            }
            accesses.push(access);
            for condition in systems.get_conditions_mut(key).into_iter().flatten() {
                accesses.push(condition.condition.initialize(world));
            }
            let filtered = accesses
                .iter()
                .filter_map(|access| match access {
                    SystemAccess::Shared(set) => Some(set),
                    SystemAccess::None | SystemAccess::Exclusive => None,
                })
                .flat_map(FilteredAccessSet::filtered_accesses);
            for filtered in filtered {
                let Some((reads, predicted)) = reads(filtered, unpredicted) else {
                    unbounded.push(name.clone());
                    continue;
                };
                for read in reads {
                    let Some((type_name, held)) = holds.get(read) else {
                        continue;
                    };
                    if !predicted {
                        read_unpredicted.push(type_name);
                    }
                    if !held.reaches(predicted) {
                        let read = format!("{name} reads {type_name} ({held:?})");
                        failures.push(read.replace("campfire_capabilities::", ""));
                    }
                }
            }
        }
    });
    // Aura carriers a client does not predict hold their modifiers on the units it does, and a
    // param reads each source's stats (10-replication-audiences.md, A client derives what it can
    // for every unit it holds).
    for type_name in [type_name::<Modifiers>(), type_name::<UnitStats>()] {
        assert!(read_unpredicted.contains(&type_name), "{type_name}");
    }
    failures.sort();
    failures.dedup();
    assert_eq!(failures, Vec::<String>::new());
    exclusive.sort();
    exclusive.dedup();
    assert_eq!(exclusive, EXCLUSIVE);
    // The server builds each system that computes its own state, and the client none: the pay of
    // ability and attack toggles, the slow weapons' hits, the hits, the damage and the deaths,
    // the forgetting of attackers, the despawns of the dead and of empty nodes, the trades, the
    // idle scans, the supply's count, the rallies, the trains' finish, the levels shown, the sight, the areas'
    // landings, at their two places, and triggers, and the projectiles' shots, launches and
    // flight.
    let mut server = World::new();
    Release::build(
        &mut server,
        spec(Role::Rules),
        books_of(&PackageSet {
            rate,
            ..every_system()
        }),
        MatchKind::Rules,
    );
    let (client, server) = (system_names(&mut world), server_only_systems(&server));
    let own = [
        "campfire_abilities::toggles::Toggles::run_toggles",
        "campfire_areas::area_life::AreaLife::land",
        "campfire_areas::area_life::AreaLife::land",
        "campfire_areas::area_life::AreaLife::trigger",
        "campfire_combat::attacks::Attacks::land_held",
        "campfire_combat::attacks::Attacks::pay_attack_toggles",
        "campfire_combat::attacks::Attacks::take_hits",
        "campfire_combat::damage_pass::DamagePass::run",
        "campfire_combat::dying::Dying::despawn_dead",
        "campfire_combat::dying::Dying::die",
        "campfire_combat::dying::Dying::forget_attackers",
        "campfire_items::trade_orders::TradeOrders::trade_items",
        "campfire_orders::acquiring::Acquiring::scan",
        "campfire_orders::attack_move::AttackMove::walk_on",
        "campfire_production::gather_loop::GatherLoop::despawn_empty_nodes",
        "campfire_production::kept_spawns::KeptSpawns::bring_back",
        "campfire_production::kept_spawns::KeptSpawns::follow_deaths",
        "campfire_production::player_supply::PlayerSupply::refresh",
        "campfire_production::production_orders::ProductionOrders::set_rallies",
        "campfire_production::trains::Trains::finish_trains",
        "campfire_progression::experience::Experience::show_levels",
        "campfire_projectiles::flying::Flying::fly",
        "campfire_projectiles::launching::Launching::launch",
        "campfire_projectiles::launching::Launching::take_shots",
        // The refresh of the unit grid before the area trigger, the hits, the flight and the
        // scan.
        "campfire_units::unit_grid::UnitGrid::refresh",
        "campfire_units::unit_grid::UnitGrid::refresh",
        "campfire_units::unit_grid::UnitGrid::refresh",
        "campfire_units::unit_grid::UnitGrid::refresh",
        "campfire_vision::seeing::Seeing::refill",
        "campfire_vision::seeing::Seeing::see",
    ];
    assert_eq!(server, own);
    // The grid's refresh runs before each of its readers, a client's among them.
    let refresh = "campfire_units::unit_grid::UnitGrid::refresh";
    for name in server.iter().filter(|name| *name != refresh) {
        let name = name.replace("campfire_capabilities::", "");
        assert!(!client.contains(&name), "{name}");
    }
    assert!(client.iter().any(|name| name == refresh));
    unbounded.sort();
    unbounded.dedup();
    assert_eq!(unbounded, Vec::<String>::new());
}
