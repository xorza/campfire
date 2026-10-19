use bevy_ecs::resource::Resource;
use bevy_reflect::TypePath;
use campfire_common::HashDomain;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelSetup;
use campfire_net_channels::ChannelTypeId;
use campfire_net_core::LinkDirections;
use campfire_net_core::NetId;
use campfire_net_core::RegistrySeal;
use campfire_net_core::TypeMapper;
use campfire_net_core::{RegistryHash, RegistryHasher};

/// The tag of the channel registry's hash.
const HASH_DOMAIN: HashDomain<RegistryHash> = HashDomain::new("campfire/channel-registry/v1");

/// The registered [`Channels`](Channel): the setup, type_id and net id of each.
///
/// ### Adding channels
///
/// You can add a new channel to the registry by calling the
/// [`register_channel`](ChannelRegistry::register_channel) method.
///
///
#[derive(Resource, Default, Clone, Debug, TypePath)]
pub struct ChannelRegistry {
    /// Each channel, at the index of its net id.
    channels: Vec<RegisteredChannel>,
    type_map: TypeMapper<ChannelTypeId>,
    seal: RegistrySeal,
}

/// A registered channel: its setup, and the directions it travels in.
#[derive(Clone, Copy, Debug)]
struct RegisteredChannel {
    setup: ChannelSetup,
    directions: LinkDirections,
}

impl ChannelRegistry {
    /// Registers the channel `C` with its setup, and returns its net id; a channel registered
    /// before keeps its first setup.
    pub fn register_channel<C: Channel>(&mut self, setup: ChannelSetup) -> NetId {
        self.open();
        if let Some(net_id) = self.net_id(ChannelTypeId::of::<C>()) {
            return net_id;
        }
        let type_id = self.type_map.add::<C>();
        self.channels.push(RegisteredChannel {
            setup,
            directions: LinkDirections::default(),
        });
        self.net_id(type_id).unwrap()
    }

    /// Widens the directions `C` travels in.
    pub(crate) fn add_direction<C: Channel>(&mut self, directions: LinkDirections) {
        self.open();
        let net_id = self
            .net_id(ChannelTypeId::of::<C>())
            .expect("a channel is registered before its direction");
        let channel = &mut self.channels[usize::from(net_id.get())];
        channel.directions = channel.directions.union(directions);
    }

    fn open(&self) {
        self.seal.assert_open("channel registry");
    }

    /// Seals the registry, as `TransportPlugin::finish` does. Its hash holds each channel's type,
    /// setup and directions, in the order of its id.
    pub(crate) fn seal(&mut self) {
        let mut hasher = RegistryHasher::new(HASH_DOMAIN);
        for ((_, name), channel) in self.type_map.iter().zip(&self.channels) {
            hasher.name(name);
            channel.setup.hash_into(&mut hasher);
            hasher.write(&[u8::from(channel.directions)]);
        }
        self.seal.seal("channel registry", hasher.finish());
    }

    /// The hash peers compare, once the registry is sealed.
    pub const fn hash(&self) -> Option<RegistryHash> {
        self.seal.hash()
    }

    /// The net id of the channel `type_id`, if it is registered.
    pub fn net_id(&self, type_id: ChannelTypeId) -> Option<NetId> {
        self.type_map.net_id(&type_id).copied()
    }

    /// The setup of the channel `net_id`, if it is registered.
    pub fn setup(&self, net_id: NetId) -> Option<&ChannelSetup> {
        self.channels
            .get(usize::from(net_id.get()))
            .map(|channel| &channel.setup)
    }

    /// The type_id of the channel `net_id`, if it is registered.
    pub fn type_id(&self, net_id: NetId) -> Option<ChannelTypeId> {
        self.type_map.type_id(net_id).copied()
    }

    /// The type name of the channel `net_id`, or `"Unknown"`.
    pub fn name(&self, net_id: NetId) -> &'static str {
        self.type_map.name_at(net_id).unwrap_or("Unknown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_channels::ChannelMode;
    use campfire_net_core::LinkDirection;
    use core::time::Duration;

    #[derive(Debug)]
    struct Chat;
    impl Channel for Chat {}

    fn hash_with(setup: ChannelSetup) -> RegistryHash {
        let mut registry = ChannelRegistry::default();
        registry.register_channel::<Chat>(setup);
        registry.seal();
        registry.hash().unwrap()
    }

    /// One channel, under each setting changed alone: every one changes the hash, so peers
    /// whose setup differ fail the protocol check.
    #[test]
    fn every_setting_enters_the_hash() {
        let base = ChannelSetup::default();
        let variants = [
            ChannelSetup {
                mode: ChannelMode::SequencedUnreliable,
                ..base
            },
            ChannelSetup {
                mode: ChannelMode::OrderedReliable,
                ..base
            },
            ChannelSetup {
                mode: ChannelMode::SequencedReliable,
                ..base
            },
            ChannelSetup {
                send_frequency: Duration::from_millis(50),
                ..base
            },
            ChannelSetup {
                priority: 2.0,
                ..base
            },
            ChannelSetup {
                retry_unsent_messages: false,
                ..base
            },
        ];
        let hashes = core::iter::once(base)
            .chain(variants)
            .map(hash_with)
            .collect::<Vec<_>>();
        for (i, hash) in hashes.iter().enumerate() {
            for other in &hashes[i + 1..] {
                assert_ne!(hash, other);
            }
        }
        assert_eq!(hashes[0], hash_with(base));
    }

    /// A channel's directions enter the hash, and not the order of the calls that set them.
    #[test]
    fn directions_enter_the_hash() {
        let hash = |directions: &[LinkDirections]| {
            let mut registry = ChannelRegistry::default();
            registry.register_channel::<Chat>(ChannelSetup::default());
            for &direction in directions {
                registry.add_direction::<Chat>(direction);
            }
            registry.seal();
            registry.hash().unwrap()
        };
        let to_client = LinkDirections::from(LinkDirection::ServerToClient);
        let to_server = LinkDirections::from(LinkDirection::ClientToServer);
        assert_ne!(hash(&[]), hash(&[to_client]));
        assert_ne!(hash(&[to_client]), hash(&[LinkDirections::BOTH]));
        assert_eq!(hash(&[to_client, to_server]), hash(&[LinkDirections::BOTH]));
    }

    #[test]
    #[should_panic(
        expected = "the channel registry takes a registration after its plugin finished"
    )]
    fn a_sealed_registry_refuses_a_channel() {
        let mut registry = ChannelRegistry::default();
        registry.seal();
        registry.register_channel::<Chat>(ChannelSetup::default());
    }
}
