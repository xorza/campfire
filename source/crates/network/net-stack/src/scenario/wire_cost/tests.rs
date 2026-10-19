use super::*;
use campfire_net_emulator::LinkModel;
use campfire_store::internals::Parallel;

/// 20 ms each way, the same both ways.
fn latency() -> LinkModel {
    LinkModel::perfect().with_latency(Duration::from_millis(20))
}

/// The cost of a direction that put `[datagrams, bytes]` on the wire, of which `[ack_only,
/// session_pings]` were the session's own, and sent `resent_messages` again.
const fn cost(wire: [u64; 2], session: [u64; 2], resent_messages: u64) -> DirectionCost {
    let [datagrams, bytes] = wire;
    let [ack_only, session_pings] = session;
    DirectionCost {
        datagrams,
        bytes,
        ack_only,
        session_pings,
        resent_messages,
    }
}

/// The counts of one second of each case, measured on the stack as it is. The stepper is
/// deterministic, so each count is exact, and a change to the wire shows as a diff of them. An
/// end whose frame is the tick sends one datagram a tick, 60 in the second, and each carries the
/// ACK and the tick, so the sessions send no packet of their own and the transports no
/// heartbeat. A sealed session adds 16 bytes of tag to
/// each datagram, and 8 of connection ID to each of the client's: 60 · 24 = 1,440 bytes more
/// from the client.
#[test]
fn each_case_puts_exact_counts_on_the_wire() {
    let unreliable = |name, client_frame, senders| WireCase {
        name,
        kind: ClientKind::Raw,
        client_frame,
        senders,
        delivery: Delivery::Unreliable,
        link: DuplexModel::symmetric(latency()),
    };
    let lossy = WireCase {
        name: "reliable under 10 ms of jitter and 2 % loss",
        kind: ClientKind::Raw,
        client_frame: TICK,
        senders: Senders::Both,
        delivery: Delivery::Reliable,
        link: DuplexModel::symmetric(
            latency()
                .with_jitter(Duration::from_millis(10))
                .with_fixed_loss(0.02),
        ),
    };
    let sealed = WireCase {
        name: "two-way at 60 Hz, sealed",
        kind: ClientKind::Udp,
        ..unreliable("", TICK, Senders::Both)
    };
    let cases = [
        (
            unreliable("two-way at 60 Hz", TICK, Senders::Both),
            [cost([60, 13353], [0, 0], 0), cost([60, 13980], [0, 0], 0)],
        ),
        (
            unreliable("a client that sends nothing", TICK, Senders::Server),
            [cost([60, 1140], [0, 0], 0), cost([60, 13980], [0, 0], 0)],
        ),
        (
            unreliable(
                "a client at 144 frames each second",
                Duration::from_nanos(6_944_444),
                Senders::Both,
            ),
            [cost([98, 13809], [0, 0], 0), cost([60, 13980], [0, 0], 0)],
        ),
        (
            lossy,
            [cost([60, 14012], [0, 0], 4), cost([60, 14307], [0, 0], 1)],
        ),
        (
            sealed,
            [cost([60, 14793], [0, 0], 0), cost([60, 14947], [0, 0], 0)],
        ),
    ];
    // The cases share no state, so they run in parallel.
    let measured = Parallel::map(cases.len(), |index| cases[index].0.measure());
    for ((case, [client_to_server, server_to_client]), measured) in cases.into_iter().zip(measured)
    {
        assert_eq!(
            measured,
            WireCost {
                client_to_server,
                server_to_client,
            },
            "{}",
            case.name
        );
    }
}
