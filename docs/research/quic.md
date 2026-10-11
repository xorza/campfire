# QUIC — research

What a transport of QUIC alone under Lightyear can be built from, read from the crates' sources and documentation, for [design 15](../design/15-quic-transport.md). Facts only: what the design takes from them is design 15's. Read on 11 October 2026; the crates are the versions in `source/Cargo.lock` on that day, Lightyear at the patched revision `9c2d69b`.

## What the build holds today

- **The chain.** `lightyear_webtransport` 0.30.1 → `aeronet_webtransport` 0.21.0 → `wtransport` 0.6.1 (HTTP/3 and WebTransport) → `quinn` 0.11.12 → `quinn-proto` 0.11.19 and `quinn-udp` 0.5.16, with `rustls` 0.23.45 over `ring` 0.17.14. `tokio` 1.53.1 comes in through `aeronet_webtransport`, `quinn` and `wtransport`, and through nothing else.
- **A runtime for each app.** `WebTransportRuntime::default` builds a multi-thread tokio runtime, one worker for each core, and leaks it (`Box::leak`); the plugin makes one in each app that adds it (`aeronet_webtransport/src/runtime.rs`).
- **Two channels for each packet.** The Bevy side hands each packet to a tokio task by an unbounded `futures` channel, and the task hands each received datagram back by another, stamped with its arrival on the tokio thread (`session.rs`, `flush`, `send_loop`, `recv_loop`). Neither channel has a bound.
- **A datagram too large is dropped.** `send_loop` takes `SendDatagramError::TooLarge` as a debug log and goes on; the packet is gone.
- **The link MTU does not follow the datagram size.** aeronet's session learns the connection's `max_datagram_size` and sets its own MTU, at least `IP_MTU`, 1,024 bytes; `lightyear_aeronet` copies packets between the session and Lightyear's `Link` and never sets the `Link`'s MTU, which stays at Lightyear's `DEFAULT_MTU`, 1,200 bytes.
- **The settings.** Lightyear's WebTransport server sets a keep-alive of 1 s and an idle timeout of 5 s, and accepts every session request (`lightyear_webtransport/src/server.rs`).
- **The exporter.** `wtransport::Connection` derives keying material from the TLS session; neither `aeronet_webtransport` nor Lightyear passes it on.

## Lightyear's IO layer

- **The link.** An IO layer is a Bevy plugin that moves bytes between a socket and the `Link` component: `Link::recv`, a queue of `BytesMut` it fills in `PreUpdate`, in `LinkReceiveSystems::BufferToLink`, and `Link::send`, a queue of `Bytes` it drains in `PostUpdate`, in `LinkSystems::Send`. Its state is a component: `Linking`, `Linked` or `Unlinked { reason }`; `LinkStart` asks it to open, `Unlink { reason }` to close. `UnlinkReason` holds text: `ByPeer(String)`, `TransportError(String)`, `UserRequested(Option<String>)` (`lightyear_link/src/lib.rs`).
- **A server's links.** A server entity holds `Server`, a relationship target; each peer is an entity of its own with `LinkOf { server }`, its `Link` and its `PeerAddr`. A server that unlinks unlinks and despawns every `LinkOf` (`lightyear_link/src/server.rs`).
- **The MTU.** A `Link` has a minimum MTU, fixed when it is made, and a current MTU, never below it; `DEFAULT_MTU` is 1,200 for both. The transport packs each packet up to the current MTU (`buffer_send`), and derives its fragment size from the minimum MTU once, when `Transport` is added to the link entity; both peers must derive the same, as the receiver joins fragments by it (`lightyear_transport/src/channel/builder.rs`, `packet/packet.rs`).
- **The UDP layer.** `lightyear_udp` binds a `std::net::UdpSocket`, non-blocking, and reads it to `WouldBlock` in each frame's `PreUpdate`, into receive buffers from a pool whose misses a test counts; the server keeps one socket and spawns a `LinkOf` for each new remote address. It has no encryption, no handshake and no congestion control.

## QUIC crates

| Crate | Form | TLS | Datagrams (RFC 9221) | Exporter | Notes |
| --- | --- | --- | --- | --- | --- |
| [`quinn-proto`](https://crates.io/crates/quinn-proto) 0.11.19 | Sans-IO state machines: `Endpoint`, `Connection` | `rustls`, over `ring` or `aws-lc-rs`; or any `crypto::Session` | `Connection::datagrams()`: `send`, `recv`, `max_size` | `crypto_session().export_keying_material` | The core of `quinn`; 355 M downloads; releases monthly in 2026 |
| [`quinn`](https://crates.io/crates/quinn) 0.11.12 | Async API over `quinn-proto` | as above | yes | yes | Needs an async runtime: tokio or smol |
| [`noq`](https://iroh.computer/blog/noq-announcement) 1.2.0 | Sans-IO `noq-proto` and an async `noq` | `rustls` by default | yes | yes | n0's hard fork of `quinn`, which runs `iroh` since 0.96: QUIC multipath, address discovery and NAT traversal |
| [`quiche`](https://docs.rs/quiche) | Sans-IO | BoringSSL, through the `boring` crates | `dgram_send`, `dgram_recv`, `dgram_max_writable_len` | none in `Connection`'s API | A C library to build |
| `s2n-quic` | Async, over its own IO provider | `s2n-tls` or `rustls` | through a provider | — | AWS's |
| `neqo` | Sans-IO | NSS | yes | — | Mozilla's, Firefox's QUIC |

## `quinn-proto` 0.11.19

- **Driving it.** `Endpoint::handle(now, remote, local_ip, ecn, data: BytesMut, buf)` takes one UDP datagram and gives a `DatagramEvent`: a new connection's `Incoming`, an event for a connection, or a response to send. A `Connection` takes `handle_event` and `handle_timeout(now)`, and gives `poll_transmit(now, max_datagrams, buf)`, `poll_timeout`, `poll` (its events: `Connected`, `ConnectionLost { reason }`, `DatagramReceived`, `DatagramsUnblocked`, …) and `poll_endpoint_events`, which go back to `Endpoint::handle_event`. Nothing in it reads a clock, a socket or a thread.
- **A new connection.** An `Incoming` is accepted (`Endpoint::accept`), refused, ignored, or answered with a stateless Retry (`Endpoint::retry`), which checks the client's address by a token before the server keeps any state for it; `Incoming::remote_address_validated` says whether its token did.
- **Datagrams.** `Datagrams::send(data: Bytes, drop)` queues a datagram within `datagram_send_buffer_size` bytes: with `drop`, it discards the oldest queued datagrams to make room; without, it returns `Blocked`. `max_size()` is `current_mtu − (1 + the peer's connection id length + 4 + the AEAD tag, 16) − 9`, the 9 being a DATAGRAM frame's type and its largest length varint; the peer's `max_datagram_frame_size` bounds it too.
- **The path MTU.** `initial_mtu` and `min_mtu` are 1,200 bytes by default, QUIC's minimum (RFC 9000 §14). Path MTU discovery sends its first probe only once the connection is established (`poll_transmit`: `self.state.is_established()`); black hole detection takes the MTU back to `min_mtu`.
- **Connection ids.** `RandomConnectionIdGenerator` makes ids of 8 bytes by default; its length is the endpoint's setting.

## `quinn-udp` 0.5.16

A blocking-free socket layer with no runtime: `UdpSocketState::new(UdpSockRef)` on a `std::net::UdpSocket` or a `socket2` socket, then `send(socket, &Transmit)` and `recv(socket, bufs, meta)`, a batch of datagrams in one call. One file for each OS family, `unix.rs`, `windows.rs` and `fallback.rs`, behind one API: it turns on ECN (`IP_RECVTOS`), the destination address of each datagram (`IP_PKTINFO`, `IPV6_RECVPKTINFO`), so a reply leaves from the address the client sent to on a socket bound to `0.0.0.0`, and GRO and GSO where Linux has them.

## `rustls` 0.23.45

- **Raw public keys** ([RFC 7250](https://www.rfc-editor.org/rfc/rfc7250)), in TLS 1.3: a server presents a key with no certificate through `server::AlwaysResolvesServerRawPublicKeys`; a client's `ServerCertVerifier` asks for one by `requires_raw_public_keys`, receives the server's `SubjectPublicKeyInfo` as the end entity, and checks the handshake's signature with `verify_tls13_signature_with_raw_key`.
- **A refusal's alert.** A verifier that refuses with `CertificateError::ApplicationVerificationFailure` makes TLS send the `access_denied` alert (`error.rs`).
- **Ed25519 keys.** The `ring` provider loads an Ed25519 PKCS#8 key. An Ed25519 key's PKCS#8 v1 is 16 fixed bytes and its 32-byte seed, and its `SubjectPublicKeyInfo` 12 fixed bytes and its 32-byte public key ([RFC 8410](https://www.rfc-editor.org/rfc/rfc8410) §7 and §10.3).
- **The exporter** ([RFC 5705](https://www.rfc-editor.org/rfc/rfc5705), [RFC 8446](https://www.rfc-editor.org/rfc/rfc8446) §7.5): `export_keying_material(output, label, context)` on a QUIC connection too. [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) defines the `tls-exporter` channel binding: the label `EXPORTER-Channel-Binding`, an empty context, 32 bytes.

## A datagram on the minimum path

On a path of 1,200 bytes, QUIC's minimum, with `quinn-proto`'s ids of 8 bytes: `1200 − (1 + 8 + 4 + 16) − 9 = 1,162` bytes of datagram. WebTransport takes one more byte for its quarter stream id on the first session, `wtransport`'s `Datagram::header_size`: 1,161. Lightyear's link packs packets of up to 1,200 bytes.

## Standards

- [RFC 9000](https://www.rfc-editor.org/rfc/rfc9000) — QUIC: the 1,200-byte minimum (§14), address validation and Retry (§8.1), connection migration (§9).
- [RFC 9001](https://www.rfc-editor.org/rfc/rfc9001) — TLS for QUIC: TLS 1.3 only, ALPN required unless another mechanism agrees on the protocol (§8.1).
- [RFC 9221](https://www.rfc-editor.org/rfc/rfc9221) — unreliable datagrams in QUIC: congestion controlled, never retransmitted.
- [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) — the `tls-exporter` channel binding, for TLS 1.3, where `tls-unique` is undefined.
- [RFC 7250](https://www.rfc-editor.org/rfc/rfc7250), [RFC 8410](https://www.rfc-editor.org/rfc/rfc8410) — raw public keys in TLS; Ed25519 keys in X.509's structures.
