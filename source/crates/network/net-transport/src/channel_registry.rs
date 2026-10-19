use bevy_ecs::resource::Resource;
use bevy_reflect::TypePath;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelConfig;
use campfire_net_channels::ChannelKind;
use campfire_net_core::Directions;
use campfire_net_core::NetId;
use campfire_net_core::RegistrySeal;
use campfire_net_core::TypeMapper;
use campfire_net_core::{RegistryHash, RegistryHasher};

/// The registered [`Channels`](Channel): the config, kind and net id of each.
///
/// ### Adding channels
///
/// You can add a new channel to the registry by calling the
/// [`register_channel`](ChannelRegistry::register_channel) method.
///
/// ```rust
/// use campfire_net_channels::{Channel, ChannelConfig, ChannelMode};
/// use campfire_net_transport::{AppChannelExt, ChannelRegistry};
/// use bevy_app::App;
/// use bevy_utils::default;
///
/// struct MyChannel;
/// impl Channel for MyChannel {}
///
/// # fn main() {
/// #  let mut app = App::new();
/// #  app.init_resource::<ChannelRegistry>();
///    app.register_channel::<MyChannel>(ChannelConfig {
///      mode: ChannelMode::UnorderedUnreliable,
///      ..default()
///    });
/// # }
/// ```
///
///
#[derive(Resource, Default, Clone, Debug, TypePath)]
pub struct ChannelRegistry {
    /// Each channel, at the index of its net id.
    channels: Vec<RegisteredChannel>,
    kind_map: TypeMapper<ChannelKind>,
    seal: RegistrySeal,
}

/// A registered channel: its config, and the directions it travels in.
#[derive(Clone, Copy, Debug)]
struct RegisteredChannel {
    config: ChannelConfig,
    directions: Directions,
}

impl ChannelRegistry {
    /// Registers the channel `C` with its config, and returns its net id; a channel registered
    /// before keeps its first config.
    pub fn register_channel<C: Channel>(&mut self, config: ChannelConfig) -> NetId {
        self.open();
        if let Some(net_id) = self.net_id(ChannelKind::of::<C>()) {
            return net_id;
        }
        let kind = self.kind_map.add::<C>();
        self.channels.push(RegisteredChannel {
            config,
            directions: Directions::default(),
        });
        self.net_id(kind).unwrap()
    }

    /// Widens the directions `C` travels in.
    pub(crate) fn add_direction<C: Channel>(&mut self, directions: Directions) {
        self.open();
        let net_id = self
            .net_id(ChannelKind::of::<C>())
            .expect("a channel is registered before its direction");
        let channel = &mut self.channels[usize::from(net_id.get())];
        channel.directions = channel.directions.union(directions);
    }

    fn open(&self) {
        self.seal.assert_open("channel registry");
    }

    /// Seals the registry, as `TransportPlugin::finish` does. Its hash holds each channel's type,
    /// config and directions, in the order of its id.
    pub(crate) fn seal(&mut self) {
        let mut hasher = RegistryHasher::default();
        for ((_, name), channel) in self.kind_map.iter().zip(&self.channels) {
            hasher.name(name);
            channel.config.hash_into(&mut hasher);
            hasher.write(&[u8::from(channel.directions)]);
        }
        self.seal.seal("channel registry", hasher.finish());
    }

    /// The hash peers compare, once the registry is sealed.
    pub const fn hash(&self) -> Option<RegistryHash> {
        self.seal.hash()
    }

    /// The net id of the channel `kind`, if it is registered.
    pub fn net_id(&self, kind: ChannelKind) -> Option<NetId> {
        self.kind_map.net_id(&kind).copied()
    }

    /// The config of the channel `net_id`, if it is registered.
    pub fn config(&self, net_id: NetId) -> Option<&ChannelConfig> {
        self.channels
            .get(usize::from(net_id.get()))
            .map(|channel| &channel.config)
    }

    /// The kind of the channel `net_id`, if it is registered.
    pub fn kind(&self, net_id: NetId) -> Option<ChannelKind> {
        self.kind_map.kind(net_id).copied()
    }

    /// The type name of the channel `net_id`, or `"Unknown"`.
    pub fn name(&self, net_id: NetId) -> &'static str {
        self.kind_map.name_at(net_id).unwrap_or("Unknown")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use campfire_net_channels::ChannelMode;
    use campfire_net_core::Direction;
    use core::time::Duration;

    #[derive(Debug)]
    struct Chat;
    impl Channel for Chat {}

    fn hash_with(config: ChannelConfig) -> RegistryHash {
        let mut registry = ChannelRegistry::default();
        registry.register_channel::<Chat>(config);
        registry.seal();
        registry.hash().unwrap()
    }

    /// One channel, under each setting changed alone: every one changes the hash, so peers
    /// whose config differ fail the protocol check.
    #[test]
    fn every_setting_enters_the_hash() {
        let base = ChannelConfig::default();
        let variants = [
            ChannelConfig {
                mode: ChannelMode::SequencedUnreliable,
                ..base
            },
            ChannelConfig {
                mode: ChannelMode::OrderedReliable,
                ..base
            },
            ChannelConfig {
                mode: ChannelMode::SequencedReliable,
                ..base
            },
            ChannelConfig {
                send_frequency: Duration::from_millis(50),
                ..base
            },
            ChannelConfig {
                priority: 2.0,
                ..base
            },
            ChannelConfig {
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
        let hash = |directions: &[Directions]| {
            let mut registry = ChannelRegistry::default();
            registry.register_channel::<Chat>(ChannelConfig::default());
            for &direction in directions {
                registry.add_direction::<Chat>(direction);
            }
            registry.seal();
            registry.hash().unwrap()
        };
        let to_client = Directions::from(Direction::ServerToClient);
        let to_server = Directions::from(Direction::ClientToServer);
        assert_ne!(hash(&[]), hash(&[to_client]));
        assert_ne!(hash(&[to_client]), hash(&[Directions::BOTH]));
        assert_eq!(hash(&[to_client, to_server]), hash(&[Directions::BOTH]));
    }

    #[test]
    #[should_panic(
        expected = "the channel registry takes a registration after its plugin finished"
    )]
    fn a_sealed_registry_refuses_a_channel() {
        let mut registry = ChannelRegistry::default();
        registry.seal();
        registry.register_channel::<Chat>(ChannelConfig::default());
    }
}
