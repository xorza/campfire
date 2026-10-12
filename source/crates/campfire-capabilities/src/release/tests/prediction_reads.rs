//! Design 14's D12: a predicting client's schedule reads, on a unit it holds as the server sent
//! it, only what it derives itself or what reaches every client that holds the unit, and builds
//! no system that computes the server's own state.

use std::collections::BTreeMap;

use bevy_ecs::component::{ComponentId, Immutable, Mutable};
use bevy_ecs::query::{ComponentAccessKind, FilteredAccess, FilteredAccessSet};
use bevy_ecs::schedule::Schedules;
use bevy_ecs::system::System;
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

    fn resource<R: SimResource + Default>(&mut self) {}

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

/// Whether a client's own player holds a type of `kind` on its entity: a system reads a player's
/// entity through the owner of a unit, and the client predicts only its own player's units, so
/// it reads its own player's (design 14, D12).
const fn of_own_player(kind: DataKind) -> bool {
    matches!(
        kind,
        DataKind::Resources | DataKind::Supply | DataKind::PlayerModifiers
    )
}

/// The systems that compute the server's own state, which a predicting client does not build;
/// the mode's `OutOfPlay::refresh`, which no client installs, among them.
const SERVER_ONLY: [&str; 7] = [
    "areas::area_life::AreaLife::trigger",
    "combat::dying::Dying::despawn_dead",
    "combat::dying::Dying::forget_attackers",
    "production::gather_loop::GatherLoop::despawn_empty_nodes",
    "production::player_supply::PlayerSupply::refresh",
    "projectiles::flying::Flying::fly",
    "vision::seeing::Seeing::see",
];

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

/// The exclusive systems a predicting client builds: Bevy gives an exclusive system no access,
/// so the check cannot read what each reads. The test fails on one this list lacks, and on one
/// it names that is gone, so no new one joins them unchecked (the issue log, **Core**).
const EXCLUSIVE: [&str; 8] = [
    "abilities::channels::Channels::predict_channels",
    "combat::attacks::Attacks::attack_events",
    "combat::damage_pass::DamagePass::run",
    "combat::modifier_intervals::ModifierIntervals::run_intervals",
    "deliveries::delivery_hooks::DeliveryHooks::end",
    "production::construction::Construction::start_builds<vision::team_sight::ByHolding>",
    "production::gather_loop::GatherLoop::run",
    "production::trains::Trains::finish_trains",
];

#[test]
fn prediction_reads_only_what_its_client_derives_or_every_client_that_holds_the_unit_receives() {
    let set = Release::capabilities();
    let mut world = World::new();
    let rate = Match::TEST_RATE;
    let spec = MatchSpec::new(SegmentSeed::new([0; 32]), rate).capabilities(set);
    Release::build(&mut world, spec.clone().predicting().unbuilt());
    let unpredicted = world.register_component::<Unpredicted>();
    let mut classes = Classes {
        world: &mut world,
        types: BTreeMap::new(),
    };
    Release::state_types(set, &mut classes);
    let types = classes.types;
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
            if system.is_exclusive() {
                exclusive.push(name.replace("campfire_capabilities::", ""));
            }
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
                        Held::Of(kind) => kind.audience() == Recipients::ALL || of_own_player(kind),
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
    assert_eq!(failures, Vec::<String>::new());
    exclusive.sort();
    exclusive.dedup();
    assert_eq!(exclusive, EXCLUSIVE);
    // The server builds each system that computes its own state, and the client none.
    let mut server = World::new();
    Release::build(&mut server, spec.unbuilt());
    let (client, server) = (system_names(&mut world), system_names(&mut server));
    for name in SERVER_ONLY {
        assert_eq!(
            [&server, &client].map(|names| names.iter().any(|built| built == name)),
            [true, false],
            "{name}"
        );
    }
    unbounded.sort();
    unbounded.dedup();
    assert_eq!(unbounded, Vec::<String>::new());
}
