use std::collections::BTreeMap;

use bevy_ecs::world::World;
use bevy_state::app::StatesPlugin;
use bevy_time::TimePlugin;
use campfire_app_threads::OneThreadPlugins;
use campfire_math::TickRate;
use campfire_net_channels::ChannelTypeId;
use campfire_net_connection::ClientOf;
use campfire_net_replication::ComponentRegistry;
use campfire_net_stack::ServerPlugins;
use campfire_net_transport::{ChannelRegistry, Transport};

use super::*;

/// Whether `registry` registers each type the lists of a set send, by name.
#[derive(Debug)]
struct Registered<'a> {
    registry: &'a ComponentRegistry,
    types: BTreeMap<&'static str, bool>,
}

impl Registered<'_> {
    fn of(registry: &ComponentRegistry, set: CapabilitySet) -> BTreeMap<&'static str, bool> {
        let mut registered = Registered {
            registry,
            types: BTreeMap::new(),
        };
        Release::state_types(set, &mut registered);
        registered.types
    }

    fn add<C: Replication>(&mut self) {
        let held = self.registry.is_registered::<C>();
        self.types.insert(C::NAME, held);
    }
}

impl StateTypes for Registered<'_> {
    fn component<C: Replication>(&mut self) {
        <C::Sending as Sending<C>>::visit(self);
    }

    fn sim_component<C: Replication>(&mut self) {
        self.component::<C>();
    }

    fn derived<C: Component>(&mut self) {}

    fn derived_sent<C: Replication + Component<Mutability = Mutable>>(&mut self) {
        self.add::<C>();
    }

    fn resource<R: SimResource + Default>(&mut self) {}
}

impl SendingWays for Registered<'_> {
    fn once<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.add::<C>();
    }

    fn on_insert<C: Replication + Component<Mutability = Immutable>>(&mut self, _: Declared) {
        self.add::<C>();
    }

    fn on_change<C: Replication + Component<Mutability = Mutable>>(&mut self, _: Declared) {
        self.add::<C>();
    }

    fn predicted<C: Replication + Component<Mutability = Mutable> + Clone + PartialEq + Debug>(
        &mut self,
        _: Declared,
    ) {
        self.add::<C>();
    }

    fn server<C: Replication>(&mut self, _: Declared) {}
}

/// A server's app with the protocol of the capabilities `set`.
fn app_of(set: CapabilitySet) -> App {
    let mut app = App::new();
    app.add_plugins((OneThreadPlugins, TimePlugin, StatesPlugin));
    app.add_plugins(ServerPlugins {
        tick_rate: TickRate::try_from(20).unwrap(),
    });
    app.add_plugins(NetProtocolPlugin { capabilities: set });
    app
}

#[test]
fn a_capability_the_mode_lacks_registers_no_component() {
    // A mode of no capability registers each type the core's lists send, and none of
    // another list's; one of every capability registers every type each list sends.
    let core = Release::capability_set(&[]).unwrap();
    let none = app_of(core);
    let registry = none.world().resource::<ComponentRegistry>();
    let own = Registered::of(registry, core);
    assert!(!own.is_empty() && own.values().all(|&held| held));
    let every = Registered::of(registry, Release::capabilities());
    assert!(every.len() > own.len());
    for (name, held) in every {
        assert_eq!(held, own.contains_key(name), "{name}");
    }
    let all = app_of(Release::capabilities());
    let registry = all.world().resource::<ComponentRegistry>();
    let every = Registered::of(registry, Release::capabilities());
    assert!(every.values().all(|&held| held));
}

/// A server's link receives on the join's channel from its start, and on the input channel
/// only once it joined, and keeps it as `Joined` goes.
#[test]
fn a_link_receives_inputs_only_once_joined() {
    let mut app = app_of(Release::capability_set(&[]).unwrap());
    let world = app.world_mut();
    let link = world.spawn((ClientOf, Transport::default())).id();
    let channels = world.resource::<ChannelRegistry>();
    let [input, join] = [
        ChannelTypeId::of::<InputChannel>(),
        ChannelTypeId::of::<JoinChannel>(),
    ]
    .map(|channel| channels.net_id(channel).unwrap());
    let receives = |world: &mut World, channel| {
        world
            .get_mut::<Transport>(link)
            .unwrap()
            .channel_receiver_mut(channel)
            .is_some()
    };
    assert_eq!(
        [receives(world, input), receives(world, join)],
        [false, true]
    );
    world.entity_mut(link).insert(Joined);
    assert_eq!(
        [receives(world, input), receives(world, join)],
        [true, true]
    );
    world.entity_mut(link).remove::<Joined>();
    assert!(receives(world, input));
}
