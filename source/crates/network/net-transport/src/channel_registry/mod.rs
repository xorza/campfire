use core::any::TypeId;

use bevy_ecs::resource::Resource;
use bevy_reflect::TypePath;
use campfire_common::HashDomain;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelReader;
use campfire_net_channels::ChannelSetup;
use campfire_net_channels::ChannelTypeId;
use campfire_net_core::LinkDirections;
use campfire_net_core::NetId;
use campfire_net_core::RegistrySeal;
use campfire_net_core::TypeMapper;
use campfire_net_core::{RegistryHash, RegistryHasher};

use crate::protocol_channel::ProtocolChannel;

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
#[derive(Resource, Clone, Debug, TypePath)]
pub struct ChannelRegistry {
    /// Each channel, at the index of its net id.
    channels: Vec<RegisteredChannel>,
    type_map: TypeMapper<ChannelTypeId>,
    seal: RegistrySeal,
}

/// A registered channel: its setup, the directions it travels in, the links that get its receiver,
/// and the layer that reads it.
#[derive(Clone, Copy, Debug)]
struct RegisteredChannel {
    setup: ChannelSetup,
    directions: LinkDirections,
    gate: ChannelGate,
    reader: ChannelReader,
}

/// Which links of a channel's receiving side get its receiver (11-net-stack/transport.md, A
/// channel opens to a link the app admits).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ChannelGate {
    /// Every link, as its transport comes.
    #[default]
    Open,
    /// A link once it holds the component of this type.
    After(TypeId),
}

/// A registered channel, as a link opens it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OpenedChannel {
    pub(crate) id: NetId,
    pub(crate) type_id: ChannelTypeId,
    pub(crate) name: &'static str,
    pub(crate) setup: ChannelSetup,
    pub(crate) reader: ChannelReader,
    pub(crate) directions: LinkDirections,
    pub(crate) gate: ChannelGate,
}

/// The registry of the protocol check's channel alone, at the first id, whichever layer
/// registers a channel first (11-net-stack/transport.md, The protocol check goes first on a
/// reserved channel).
impl Default for ChannelRegistry {
    fn default() -> Self {
        let mut registry = Self {
            channels: Vec::new(),
            type_map: TypeMapper::default(),
            seal: RegistrySeal::default(),
        };
        let id = registry.register_channel::<ProtocolChannel>(ProtocolChannel::setup());
        debug_assert_eq!(id, ProtocolChannel::ID, "the check's channel is the first");
        registry.add_direction::<ProtocolChannel>(LinkDirections::BOTH, ChannelGate::Open);
        registry.read_by_owner::<ProtocolChannel>();
        registry
    }
}

impl ChannelRegistry {
    /// Its name, in its seal's messages and in the protocol check, where it names its hash.
    pub(crate) const NAME: &'static str = "channel registry";

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
            gate: ChannelGate::default(),
            reader: ChannelReader::default(),
        });
        self.net_id(type_id).unwrap()
    }

    /// Widens the directions `C` travels in, its receiver behind `gate`: one gate for every
    /// direction of a channel.
    pub(crate) fn add_direction<C: Channel>(
        &mut self,
        directions: LinkDirections,
        gate: ChannelGate,
    ) {
        self.open();
        let net_id = self
            .net_id(ChannelTypeId::of::<C>())
            .expect("a channel is registered before its direction");
        let channel = &mut self.channels[usize::from(net_id.get())];
        assert!(
            channel.directions.is_empty() || channel.gate == gate,
            "channel {} opens behind two gates",
            core::any::type_name::<C>()
        );
        channel.directions = channel.directions.union(directions);
        channel.gate = gate;
    }

    /// Whether a channel opens behind `gate`.
    pub(crate) fn gates(&self, gate: ChannelGate) -> bool {
        self.channels
            .iter()
            .any(|channel| !channel.directions.is_empty() && channel.gate == gate)
    }

    /// Each channel that travels in a direction, as a link opens it.
    pub(crate) fn channels(&self) -> impl Iterator<Item = OpenedChannel> + '_ {
        self.type_map
            .iter()
            .zip(&self.channels)
            .zip(0..)
            .filter(|((_, channel), _)| !channel.directions.is_empty())
            .map(|((&(type_id, name), channel), id)| OpenedChannel {
                id: NetId::new(id),
                type_id,
                name,
                setup: channel.setup,
                reader: channel.reader,
                directions: channel.directions,
                gate: channel.gate,
            })
    }

    /// Makes the layer that registered `C` its reader.
    pub(crate) fn read_by_owner<C: Channel>(&mut self) {
        self.open();
        let net_id = self
            .net_id(ChannelTypeId::of::<C>())
            .expect("a channel is registered before its reader");
        self.channels[usize::from(net_id.get())].reader = ChannelReader::Owner;
    }

    fn open(&self) {
        self.seal.assert_open(Self::NAME);
    }

    /// Seals the registry, as `TransportPlugin::finish` does, once each channel's bounds hold. Its
    /// hash holds each channel's type, setup and directions, in the order of its id.
    pub(crate) fn seal(&mut self) {
        let mut hasher = RegistryHasher::new(HASH_DOMAIN);
        for ((_, name), channel) in self.type_map.iter().zip(&self.channels) {
            channel.setup.assert_bounds();
            hasher.name(name);
            channel.setup.hash_into(&mut hasher);
            hasher.write(&[u8::from(channel.directions)]);
        }
        self.seal.seal(Self::NAME, hasher.finish());
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
mod tests;
