# Campfire — QUIC transport

The server and the client link over QUIC alone, through `quic`, an IO layer on `quinn-proto` under Lightyear's `Link`. QUIC alone, not WebTransport: WebTransport is HTTP/3 over QUIC, and its one gain is that a browser reaches it, which Campfire dropped ([Code](../../AGENTS.md#code)). The facts behind each decision that stands are in [UDP research](../research/udp.md).

## The stack

- **QUIC on `quinn-proto`, in its own crate.** `quinn-proto` is QUIC with no IO, over `rustls` and `ring`, with RFC 9221's datagrams and TLS's exporter. Not `quinn`, whose API needs an async runtime; not `noq`, whose multipath no slice needs; not `quiche`, a C library with no exporter; not `s2n-quic` or `neqo`, with their own IO or NSS; not Lightyear's UDP, with no encryption or congestion control. `quic` names no game type, and `net` keeps what is Campfire's.
- **The socket is `quinn-udp`'s.** It reads and writes in batches, turns on ECN, and gives each datagram's destination, so a server bound to `0.0.0.0` replies from the address the client reached.
- **A thread for each endpoint, and no IO in the frame.** The thread waits through `polling` for the socket, a deadline or a wake, reads one batch with no lock and holds the shared mutex only for that batch, so acknowledgments and keep-alives go on through a stalled frame, and a frame waits at most one batch. The wake is a loopback datagram, as the self-pipe trick does, as `polling`'s own `notify` allocates on each call on kqueue.
- **Datagrams, not streams.** One Lightyear packet is one DATAGRAM frame: Lightyear keeps reliability, order and fragments, QUIC keeps encryption, congestion control and the path MTU; a full send queue drops its oldest datagram, as the newest state matters most.
- **A message's size is declared, planned.** Each registered message type declares its largest encoding in `net_protocol`, and the transport refuses a fragment count past it before it allocates, so a peer's count sizes nothing. Refused: a bound in the fork alone, which knows no message's size.
- **A link's minimum MTU is QUIC's datagram.** `1200 − (1 + 8 + 4 + 16) − 9 = 1,162` bytes on the minimum path, for every link, in-process ones too, as both ends must agree on Lightyear's fragment size.

## The server's key

- **A raw key derived from the server's key.** The server presents an Ed25519 key with no certificate (RFC 7250), as no client reads a certificate's names or dates; its seed is HKDF-SHA256 of the server's own key, so one secret file holds both and the pin cannot be lost while the identity stays.
- **TLS 1.3, ALPN `campfire`, no client key, no resumption.** A player proves its key by the connect answer, not by TLS; with no session ticket, every handshake presents the key and the verifier checks it.

## Connections

- **Retry, tokens and resets after a restart.** A stateless Retry for each new address (RFC 9000 §8.1), so a spoofed address costs no state; tokens bound to the client's IP skip the Retry within a restore window, each once; 0-RTT would gain no round trip, as the connect answer signs the exporter only a completed handshake gives.
- **A link's entity is its peer, not its address,** as QUIC holds two connections from one address and moves one to another; a link entity's removal, by any path, closes its connection.
- **A link's end is a type.** A link holds how it ended, an IO loss or the code its peer closed it with, so a peer that ends a link ends the other at once and says why.
- **Limits for each address, from the session's seats.** No address of honest players needs more live connections than the session has seats, so an IPv4 address or an IPv6 /64 holds at most that many, and its handshakes come from a bucket of the same size, as the generic cell rate algorithm keeps a token bucket in one number.
- **A flood of handshakes is bounded, measured, and met by a puzzle only when needed.** A pass takes a bounded count of new connections; a puzzle comes only once a bench shows a flood within an attacker's reach starves the server, and then as IKEv2's and Tor's are, dormant at effort 0 and raised under load.

## Reachability

- **A server maps its own port.** On a thread of its own, PCP's `MAP` to the default gateway, then NAT-PMP, then UPnP-IGD, the first that answers winning within seconds; the host forwards it by hand where no router answers.

## The connect answer

- **The answer binds to the TLS session.** The answer signs RFC 9266's `tls-exporter` binding, so a party that holds the server's TLS key and relays an answer holds two sessions, whose bindings differ, and the relay fails.
