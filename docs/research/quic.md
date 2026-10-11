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
- **When the fragment size is fixed.** `Transport`'s `on_add` hook reads the `Link`'s minimum MTU once; a `Link` changed after it keeps the old fragment size. `Client` requires `PingManager`, which requires `MessageSender<Ping>`, then `MessageManager`, `Transport` and `Link`, so a client entity's fragment size is fixed when the entity is spawned, from the `Link` the spawn holds or `DEFAULT_MTU`. Campfire spawns it in `SimClient::spawn_client`, before `Connection::link` inserts its IO; the harness and the local server spawn each in-process `LinkOf` with `Link::default()`.
- **A raw peer's id.** `RawConnectionPlugin`'s client keeps a `RemoteId` its entity holds; its server inserts `RemoteId(PeerId::Raw(PeerAddr))` on each `LinkOf` once it is linked, and keeps none it holds (`lightyear_raw_connection/src/server.rs`). `Connected` puts the `RemoteId` in `NetworkingMetadata`'s peer map, and a second link of the same id takes the first one's place there. Replication gives each connected link replicon's `NetworkId` from `RemoteId::to_bits`, which for `PeerId::Raw` packs an IPv4 address and its port and folds an IPv6 one by XOR (`lightyear_core/src/id.rs`, `lightyear_replication/src/server.rs`).
- **A server's links at its end.** A server entity that becomes `Unlinked` triggers `Unlink` on each `LinkOf` and despawns it in the same command flush (`Server::unlinked`).
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
- **Connection ids.** `RandomConnectionIdGenerator` makes ids of 8 bytes by default; its length is the endpoint's setting. `max_size` reads the length of the id the peer gave, `predict_1rtt_overhead`, so it follows the peer's choice: an id of 20 bytes, QUIC's largest, makes it 1,150 on the minimum path. A peer may give ids of other lengths during the connection (RFC 9000 §5.1.1).
- **The transport's defaults** (`TransportConfig::default`): the peer may open 100 bidirectional and 100 unidirectional streams, with a stream receive window of 1,250,000 bytes and a connection receive window of `VarInt::MAX`; the datagram receive buffer is 1,250,000 bytes, and the send buffer 1 MiB; no keep-alive; an idle timeout of 30 s; an initial round trip of 333 ms.
- **Validation tokens.** With the `bloom` feature, a server sends 2 `NEW_TOKEN` tokens after each handshake, which let the client's next connection skip the Retry, and keeps a Bloom filter of the tokens used; without it, `ValidationTokenConfig::default` sends none and accepts none (`NoneTokenLog`), and only a Retry's token validates an address.
- **The stateless reset key.** `EndpointConfig::default` draws a new 64-byte HMAC key for each endpoint; a reset token is that key's HMAC of a connection id. RFC 9000 §10.3 has an endpoint that wants to reset connections after it lost their state derive the tokens from a static key.

## `quinn-udp` 0.5.16

A blocking-free socket layer with no runtime: `UdpSocketState::new(UdpSockRef)` on a `std::net::UdpSocket` or a `socket2` socket, then `send(socket, &Transmit)` and `recv(socket, bufs, meta)`, a batch of datagrams in one call. One file for each OS family, `unix.rs`, `windows.rs` and `fallback.rs`, behind one API: it turns on ECN (`IP_RECVTOS`), the destination address of each datagram (`IP_PKTINFO`, `IPV6_RECVPKTINFO`), so a reply leaves from the address the client sent to on a socket bound to `0.0.0.0`, and GRO and GSO where Linux has them. `UdpSocketState::new` makes the socket non-blocking on each OS family, and `recv` gives no error but `WouldBlock`, so a caller that waits for a datagram waits on the socket's readiness by other means.

## Waiting with no runtime

- **`quinn`'s shape.** `quinn` keeps an endpoint's state, and each connection's, behind a `Mutex` (`EndpointInner`, `ConnectionInner`); a driver task takes it to read the socket and serve the timers, and the application's handles take it to queue data, then wake the driver (`quinn/src/endpoint.rs`, `connection.rs`).
- **`polling` 3.11.0**, in the build through `async-io` and `bevy_asset`: one API over epoll, kqueue and Windows' IOCP. `Poller::wait(events, timeout)` blocks until a source it watches is ready or the timeout passes, and `Poller::notify` wakes a wait from another thread. `mio`, the other such crate, comes in only through tokio.

## `rustls` 0.23.45

- **Raw public keys** ([RFC 7250](https://www.rfc-editor.org/rfc/rfc7250)), in TLS 1.3: a server presents a key with no certificate through `server::AlwaysResolvesServerRawPublicKeys`; a client's `ServerCertVerifier` asks for one by `requires_raw_public_keys`, receives the server's `SubjectPublicKeyInfo` as the end entity, and checks the handshake's signature with `verify_tls13_signature_with_raw_key`.
- **A refusal's alert.** A verifier that refuses with `CertificateError::ApplicationVerificationFailure` makes TLS send the `access_denied` alert (`error.rs`).
- **Ed25519 keys.** The `ring` provider loads an Ed25519 PKCS#8 key. An Ed25519 key's PKCS#8 v1 is 16 fixed bytes and its 32-byte seed, and its `SubjectPublicKeyInfo` 12 fixed bytes and its 32-byte public key ([RFC 8410](https://www.rfc-editor.org/rfc/rfc8410) §7 and §10.3).
- **Features.** `rustls`'s default features are `aws_lc_rs`, `prefer-post-quantum` (which needs `aws_lc_rs`), `tls12`, `logging` and `std`. `quinn-proto`'s are `rustls-ring`, `log` and `bloom`. No crate of the build holds `aws-lc-rs` today.
- **Resumption.** A `ServerConfig` sends 2 TLS 1.3 tickets after each handshake by default (`send_tls13_tickets`), into an in-memory cache; a `ClientConfig` resumes by default. A resumed handshake sends no `Certificate`, and the client's verifier is not called. Early data is off while `max_early_data_size` is 0, the default.
- **The exporter** ([RFC 5705](https://www.rfc-editor.org/rfc/rfc5705), [RFC 8446](https://www.rfc-editor.org/rfc/rfc8446) §7.5): `export_keying_material(output, label, context)` on a QUIC connection too. [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) defines the `tls-exporter` channel binding: the label `EXPORTER-Channel-Binding`, an empty context, 32 bytes.

## A datagram on the minimum path

On a path of 1,200 bytes, QUIC's minimum, with `quinn-proto`'s ids of 8 bytes: `1200 − (1 + 8 + 4 + 16) − 9 = 1,162` bytes of datagram. WebTransport takes one more byte for its quarter stream id on the first session, `wtransport`'s `Datagram::header_size`: 1,161. Lightyear's link packs packets of up to 1,200 bytes.

## Port mapping

Read on 12 October 2026.

- **Three protocols.** A host behind a home router opens an inbound port by asking its gateway: UPnP-IGD, SSDP discovery by multicast and SOAP over HTTP to the router; NAT-PMP ([RFC 6886](https://www.rfc-editor.org/rfc/rfc6886)), fixed UDP messages to the gateway's port 5351; and PCP ([RFC 6887](https://www.rfc-editor.org/rfc/rfc6887)), NAT-PMP's successor on the same port.
- **PCP's MAP.** A request is 60 bytes: a header of 24 (version 2, opcode 1 with R clear, a requested lifetime in seconds, the client's IP as IPv4-mapped IPv6) and a body of 36 (a 96-bit nonce, protocol 17 for UDP, the internal port, a suggested external port, a suggested external IP, all zeros for none). The response has the same sizes: a result code, the granted lifetime, the epoch, the assigned external port and IP (§7, §11.1). A client renews at a random point in 1/2 to 5/8 of the lifetime, then 3/4, 7/8, never twice within 4 s (§11.2.1); a request of lifetime 0 with the same protocol and internal port deletes the mapping (§15). A NAT-PMP-only gateway answers `UNSUPP_VERSION` with version 0, and the client may then speak NAT-PMP (§9). The first retransmission comes after 3 s, each later one after twice the last, up to 1,024 s, with no limit of tries by default (§8.1.1).
- **NAT-PMP.** A mapping request is 12 bytes: version 0, opcode 1 for UDP, the internal port, a suggested external port and a lifetime; the response is 16: result code, epoch, the internal port, the mapped external port and the granted lifetime. The external address is a separate request, opcode 0, whose response of 12 bytes holds it. The recommended lifetime is 7,200 s ([RFC 6886](https://www.rfc-editor.org/rfc/rfc6886) §3.3).
- **A router's reply from another port.** TP-Link's Omada ER605, as tested on 12 October 2026, answers an SSDP search to the searcher's address and port, but from a port of its own choice, not 1900, which UDA leaves open; a host firewall that tracks connections, as UFW does, matches that reply to no request it sent and drops it. Its `NOTIFY` announcements, multicast to 239.255.255.250:1900, which UFW's default rules accept, carry the same location. Its description is at a path of its own, `/igd<number>.xml`, on port 1900. It answers PCP's port with ICMP: no PCP or NAT-PMP.
- **Crates.** [`portmapper`](https://docs.rs/crate/portmapper/latest) 0.19.3, iroh's, does all three, on tokio, with `netwatch`, `hyper-util` and `iroh-metrics`. [`igd-next`](https://docs.rs/crate/igd-next/latest) 0.18.0 does UPnP-IGD with a blocking API by default (`io_sync`, through `attohttpc`), tokio optional: `search_gateway`, `Gateway::add_port(protocol, external_port, local_addr, lease_duration, description)`, `remove_port`, `get_external_ip`; `AddPortError::OnlyPermanentLeasesSupported` names a router that takes only a lease of 0, which never ends. [`natpmp`](https://docs.rs/crate/natpmp/latest) 0.5.0 does NAT-PMP alone, with tokio by default; [`crab_nat`](https://docs.rs/crab_nat) does PCP and NAT-PMP on tokio. [`netdev`](https://docs.rs/crate/netdev/latest) 0.46.3, MIT, reads an interface's default gateway on each OS behind one API (netlink on Linux, `IpHelper` on Windows, `SystemConfiguration` on macOS).

## Standards

- [RFC 9000](https://www.rfc-editor.org/rfc/rfc9000) — QUIC: the 1,200-byte minimum (§14), address validation and Retry (§8.1), connection migration (§9).
- [RFC 9001](https://www.rfc-editor.org/rfc/rfc9001) — TLS for QUIC: TLS 1.3 only, ALPN required unless another mechanism agrees on the protocol (§8.1).
- [RFC 9221](https://www.rfc-editor.org/rfc/rfc9221) — unreliable datagrams in QUIC: congestion controlled, never retransmitted.
- [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) — the `tls-exporter` channel binding, for TLS 1.3, where `tls-unique` is undefined.
- [RFC 7250](https://www.rfc-editor.org/rfc/rfc7250), [RFC 8410](https://www.rfc-editor.org/rfc/rfc8410) — raw public keys in TLS; Ed25519 keys in X.509's structures.
- [RFC 6886](https://www.rfc-editor.org/rfc/rfc6886), [RFC 6887](https://www.rfc-editor.org/rfc/rfc6887) — NAT-PMP and PCP: a gateway's port mappings by UDP.
