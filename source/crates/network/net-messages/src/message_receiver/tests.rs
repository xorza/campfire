use super::*;
use crate::message_receive_cx::ReceivedMessageHeader;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelTypeId;
use campfire_net_core::NetTick;

#[derive(Debug, PartialEq)]
struct TestMessage(&'static str);

#[derive(Debug)]
struct TestChannel;
impl Channel for TestChannel {}

#[test]
fn received_messages_keep_their_order_and_metadata() {
    let mut receiver = MessageReceiver::<TestMessage>::default();
    for (message, tick) in [("first", 10), ("second", 10), ("third", 11)] {
        receiver.push_received(ReceivedMessage {
            data: TestMessage(message),
            header: ReceivedMessageHeader {
                remote_tick: NetTick(tick),
                channel_type_id: ChannelTypeId::of::<TestChannel>(),
                message_id: None,
            },
        });
    }

    let received = receiver.receive_with_tick().collect::<Vec<_>>();
    assert_eq!(
        received
            .iter()
            .map(|m| (m.data.0, m.header.remote_tick))
            .collect::<Vec<_>>(),
        vec![
            ("first", NetTick(10)),
            ("second", NetTick(10)),
            ("third", NetTick(11))
        ]
    );
    assert!(receiver.is_empty());
}
