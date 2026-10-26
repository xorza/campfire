use super::*;
use campfire_net_emulator::LinkModel;
use campfire_net_emulator::internals::per_mille;
use campfire_threads::Parallel;

/// 20 ms each way, the same both ways.
fn latency() -> LinkModel {
    LinkModel::perfect().with_latency(Duration::from_millis(20))
}

/// The cost of a direction that put `[datagrams, bytes]` on the wire, of which `[ack_only,
/// session_pings]` were the packet_session's own, and sent `resent_messages` again.
const fn cost(wire: [u64; 2], packet_session: [u64; 2], resent_messages: u64) -> DirectionCost {
    let [datagrams, bytes] = wire;
    let [ack_only, packet_session_pings] = packet_session;
    DirectionCost {
        datagrams,
        bytes,
        ack_only,
        packet_session_pings,
        resent_messages,
    }
}

/// The counts of one second of each case, measured on the stack as it is. The stepper is
/// deterministic, so each count is exact, and a change to the wire shows as a diff of them. An
/// end whose frame is the tick sends one datagram a tick, 60 in the second, and each carries the
/// ACK and the tick, so the packet sessions send no packet of their own and the transports no
/// heartbeat. A sealed packet session adds 16 bytes of tag to
/// each datagram, and 8 of connection ID to each of the client's: 60 · 24 = 1,440 bytes more
/// from the client.
#[test]
fn each_case_puts_exact_counts_on_the_wire() {
    let unreliable = |name, client_frame, senders| WireCase {
        name,
        kind: ClientKind::Plain,
        client_frame,
        senders,
        delivery: Reliability::Unreliable,
        link: DuplexModel::symmetric(latency()),
    };
    let lossy = WireCase {
        name: "reliable under 10 ms of jitter and 2 % loss",
        kind: ClientKind::Plain,
        client_frame: TICK,
        senders: Senders::Both,
        delivery: Reliability::Reliable,
        link: DuplexModel::symmetric(
            latency()
                .with_jitter(Duration::from_millis(10))
                .with_fixed_loss(per_mille(20)),
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
            [cost([60, 13653], [0, 0], 0), cost([60, 14280], [0, 0], 0)],
        ),
        (
            unreliable("a client that sends nothing", TICK, Senders::Server),
            [cost([60, 1440], [0, 0], 0), cost([60, 14280], [0, 0], 0)],
        ),
        (
            unreliable(
                "a client at 144 frames each second",
                Duration::from_nanos(6_944_444),
                Senders::Both,
            ),
            [cost([95, 14041], [0, 0], 0), cost([60, 14280], [0, 0], 0)],
        ),
        (
            lossy,
            [cost([60, 14103], [0, 0], 4), cost([60, 14607], [0, 0], 1)],
        ),
        (
            sealed,
            [cost([60, 15093], [0, 0], 0), cost([60, 15247], [0, 0], 0)],
        ),
    ];
    // The cases share no state, so they run in parallel; one compare of them all shows every
    // count a change moved.
    let measured = Parallel::map(cases.len(), |index| cases[index].0.measure());
    let measured: Vec<_> = (cases.iter().map(|(case, _)| case.name))
        .zip(measured)
        .collect();
    let expected: Vec<_> = cases
        .iter()
        .map(|&(ref case, [client_to_server, server_to_client])| {
            let cost = WireCost {
                client_to_server,
                server_to_client,
            };
            (case.name, cost)
        })
        .collect();
    assert_eq!(measured, expected);
}
