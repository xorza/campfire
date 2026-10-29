use core::time::Duration;

use campfire_net_channels::ChannelMode;
use campfire_net_core::LinkDirection;

use super::*;

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
            registry.add_direction::<Chat>(direction, ChannelGate::Open);
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

/// A channel is the message layer's to read until its own layer takes it; the reader stays out
/// of the hash, as it changes no byte between peers.
#[test]
fn a_channel_is_read_by_messages_until_its_owner_takes_it() {
    let mut registry = ChannelRegistry::default();
    let chat = ChannelTypeId::of::<Chat>();
    assert_eq!(registry.reader(chat), None);
    registry.register_channel::<Chat>(ChannelSetup::default());
    assert_eq!(registry.reader(chat), Some(ChannelReader::Messages));
    let mut owned = registry.clone();
    owned.read_by_owner::<Chat>();
    assert_eq!(owned.reader(chat), Some(ChannelReader::Owner));
    registry.seal();
    owned.seal();
    assert_eq!(registry.hash(), owned.hash());
}

#[test]
#[should_panic(expected = "the channel registry takes a registration after its plugin finished")]
fn a_sealed_registry_refuses_a_channel() {
    let mut registry = ChannelRegistry::default();
    registry.seal();
    registry.register_channel::<Chat>(ChannelSetup::default());
}

/// A channel whose largest message the receive budget does not hold fails the seal.
#[test]
#[should_panic(expected = "pass the receive budget")]
fn a_channel_past_the_receive_budget_fails_the_seal() {
    hash_with(ChannelSetup {
        max_message_len: usize::MAX,
        ..ChannelSetup::default()
    });
}

impl ChannelRegistry {
    /// The layer that reads the channel `type_id`, if it is registered.
    fn reader(&self, type_id: ChannelTypeId) -> Option<ChannelReader> {
        let net_id = self.net_id(type_id)?;
        Some(self.channels[usize::from(net_id.get())].reader)
    }
}
