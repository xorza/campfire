# UDP — research

What the stack's sealed UDP and its port map are built from, read from the crates' sources, the standards and a router, for [UDP](../design/11-net-stack/udp.md) and [Port mapping](../design/11-net-stack/port-map.md). Facts only: what the design takes from them is its own. The crates are the versions in `source/Cargo.lock` on the day each section names.

## `quinn-udp` 0.6.3

Read on 20 October 2026. A socket layer with no runtime: `UdpSocketState::new(UdpSockRef)` on a `std::net::UdpSocket` or a `socket2` socket, then `send(socket, &Transmit)` and `recv(socket, bufs, meta)`, a batch of datagrams in one call. `unix.rs`, with `linux.rs` and `apple_fast.rs` beside it, and `windows.rs` give one API: it turns on ECN (`IP_RECVTOS`), the destination address of each datagram (`IP_PKTINFO`, `IPV6_RECVPKTINFO`), so a reply leaves from the address the client sent to on a socket bound to `0.0.0.0`, and GRO and GSO where Linux has them. `UdpSocketState::new` makes the socket non-blocking on each OS family, so a caller that waits for a datagram waits on the socket's readiness by other means.

## `polling` 3.11.0

Read on 11 October 2026. One API over epoll, kqueue and Windows' IOCP. `Poller::wait(events, timeout)` blocks until a source it watches is ready or the timeout passes, and `Poller::notify` wakes a wait from another thread. `mio`, the other such crate, comes in only through tokio.

## A wake in flight

Measured on 26 October 2026, on an M5 Mac and on the Linux workstation, with a probe of the steps of `udp_port`'s wake test, as macOS CI run 40353599661 failed its second assertion.

- **A loopback datagram arrives after its send.** On macOS a datagram from one loopback socket to another may reach the receiver after `send` returns, so a wait that drains its socket may leave a byte still in flight, which ends the next wait.
- **A byte for each wake.** The probe failed 4 rounds of 20,000 at rest and about 1 in 19 under CPU load on the M5, and none of 160,000 on Linux under load.
- **One byte while a flag is pending** (commit `ff9e1346`): none of 180,000 rounds failed on the M5. With the flag cleared by an exchange that acquires and releases, none of 20,000 rounds failed at rest and none of 100,000 under load, twelve `yes` processes on its ten cores.
- **Others coalesce.** libuv's `uv__async_io` (`src/unix/async.c`, branch `v1.x`, read the same day) reads its pipe, outside Linux, and then clears each handle's `pending` by an atomic `fetch_and`, which its comment pairs with the sequentially consistent compare-and-swap of `uv_async_send`. `polling` 3.11.0's `Poller::notify` notifies only when a sequentially consistent compare-exchange of its `notified` finds it clear, and `Poller::wait` clears it by a sequentially consistent swap after its wait.

## A wake that fails

Read on 26 October 2026.

- **tokio** (`tokio/src/runtime/io/driver.rs`, branch `master`): `Handle::unpark` calls `self.waker.wake().expect("failed to wake I/O driver")`, so a failed wake of the I/O driver panics; no other call wakes its `mio` waker.
- **libuv** (`src/unix/async.c`, branch `v1.x`): `uv__async_send` writes to its eventfd on Linux, and on a full counter drains it and writes again; triggers an `EVFILT_USER` event where kqueue has it; and elsewhere writes one byte to its pipe, again while the write fails with `EINTR`, and returns on `EAGAIN` or `EWOULDBLOCK`. Any other failure reaches `abort()`.
- **polling** (`src/lib.rs`, branch `master`): `Poller::notify` sets `notified` by a compare-exchange and only then calls the backend's notify, returning its error with `notified` still set; `wait_impl` clears it by a swap. A failed notify so leaves every later notify a no-op until a wait ends by other means.

## `rustls` 0.23.45

Read on 11 October 2026.

- **Raw public keys** ([RFC 7250](https://www.rfc-editor.org/rfc/rfc7250)), in TLS 1.3: a server presents a key with no certificate through `server::AlwaysResolvesServerRawPublicKeys`; a client's `ServerCertVerifier` asks for one by `requires_raw_public_keys`, receives the server's `SubjectPublicKeyInfo` as the end entity, and checks the handshake's signature with `verify_tls13_signature_with_raw_key`.
- **A refusal's alert.** A verifier that refuses with `CertificateError::ApplicationVerificationFailure` makes TLS send the `access_denied` alert (`error.rs`).
- **Ed25519 keys.** The `ring` provider loads an Ed25519 PKCS#8 key. An Ed25519 key's PKCS#8 v1 is 16 fixed bytes and its 32-byte seed, and its `SubjectPublicKeyInfo` 12 fixed bytes and its 32-byte public key ([RFC 8410](https://www.rfc-editor.org/rfc/rfc8410) §7 and §10.3).
- **Features.** `rustls`'s default features are `aws_lc_rs`, `prefer-post-quantum` (which needs `aws_lc_rs`), `tls12`, `logging` and `std`.
- **Resumption.** A `ServerConfig` sends 2 TLS 1.3 tickets after each handshake by default (`send_tls13_tickets`), into an in-memory cache; a `ClientConfig` resumes by default. A resumed handshake sends no `Certificate`, and the client's verifier is not called. Early data is off while `max_early_data_size` is 0, the default.
- **The exporter** ([RFC 5705](https://www.rfc-editor.org/rfc/rfc5705), [RFC 8446](https://www.rfc-editor.org/rfc/rfc8446) §7.5): `export_keying_material(output, label, context)`, on a QUIC connection too. [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) defines the `tls-exporter` channel binding: the label `EXPORTER-Channel-Binding`, an empty context, 32 bytes.

## Port mapping

Read on 12 October 2026.

- **Three protocols.** A host behind a home router opens an inbound port by asking its gateway: UPnP-IGD, SSDP discovery by multicast and SOAP over HTTP to the router; NAT-PMP ([RFC 6886](https://www.rfc-editor.org/rfc/rfc6886)), fixed UDP messages to the gateway's port 5351; and PCP ([RFC 6887](https://www.rfc-editor.org/rfc/rfc6887)), NAT-PMP's successor on the same port.
- **A router's reply from another port.** TP-Link's Omada ER605, as tested on 12 October 2026, answers an SSDP search to the searcher's address and port, but from a port of its own choice, not 1900, which UDA leaves open; a host firewall that tracks connections, as UFW does, matches that reply to no request it sent and drops it. Its `NOTIFY` announcements, multicast to 239.255.255.250:1900, which UFW's default rules accept, carry the same location. Its description is at a path of its own, `/igd<number>.xml`, on port 1900. It answers PCP's port with ICMP: no PCP or NAT-PMP.
- **Crates.** [`portmapper`](https://docs.rs/crate/portmapper/latest) 0.19.3, iroh's, does all three, on tokio, with `netwatch`, `hyper-util` and `iroh-metrics`. [`igd-next`](https://docs.rs/crate/igd-next/latest) 0.18.0 does UPnP-IGD with a blocking API by default (`io_sync`, through `attohttpc`), tokio optional: `search_gateway`, `Gateway::add_port(protocol, external_port, local_addr, lease_duration, description)`, `remove_port`, `get_external_ip`; `AddPortError::OnlyPermanentLeasesSupported` names a router that takes only a lease of 0, which never ends. [`natpmp`](https://docs.rs/crate/natpmp/latest) 0.5.0 does NAT-PMP alone, with tokio by default; [`crab_nat`](https://docs.rs/crab_nat) does PCP and NAT-PMP on tokio. [`netdev`](https://docs.rs/crate/netdev/latest) 0.46.3, MIT, reads an interface's default gateway on each OS behind one API (netlink on Linux, `IpHelper` on Windows, `SystemConfiguration` on macOS).

## Client puzzles

Read on 12 October 2026.

- **IKEv2** ([RFC 8019](https://www.rfc-editor.org/rfc/rfc8019)): a responder escalates, a stateless cookie for every initiator first, then puzzles, then hard limits, and puzzles for all initiators last, "the last step, and the Responder should avoid this if possible"; the countermeasures "SHOULD NOT be deployed unless an attack is likely to be in progress" (§6). A puzzle is four keys whose PRF of the cookie has enough trailing zero bits (§7.1.3), about a second of work, 20 bits (§9). The responder ranks requests by the work proved and serves the high first, a request with none the lowest (§7.1.4). It limits half-open exchanges for each IPv4 address or each IPv6 /48 or /64, softly by a puzzle or hard, and warns that a hard limit for each address shuts out the honest users behind one NAT (§4.2).
- **Tor's onion services** ([proposal 327](https://spec.torproject.org/proposals/327-pow-over-intro.html), in Tor 0.4.8 of 2023): the effort a service suggests starts at 0, "dormant until we notice signs of overload", and a control loop raises and lowers it ([proposal 362](https://spec.torproject.org/proposals/362-update-pow-control-loop.html)); valid requests wait in a queue by effort, those under the suggested effort too, and an invalid proof is ignored. The puzzle is Equi-X, "lightning fast verification speed", which "aims to minimize the asymmetry between CPU and GPU"; a rotating seed and a cache of (seed, nonce) stop a replay. The proposal says that "a simple PoW proposal cannot defend against all and every DoS attack on the Internet".
- **TLS** ([draft-venhoek-tls-client-puzzles](https://www.ietf.org/archive/id/draft-venhoek-tls-client-puzzles-00.html), November 2024, after [draft-nygren](https://datatracker.ietf.org/doc/draft-nygren-tls-client-puzzles/)): a puzzle in TLS 1.3's HelloRetryRequest, which the client solves before it sends its ClientHello again; a draft, which `rustls` does not implement. Nygren's draft grants puzzles are "unfair" to clients of little power, mobile ones, and calls them only under attack.
- **What a puzzle cannot stop.** A bot sends its flood up to its bandwidth, not its CPU, and a GPU solves a hash puzzle many times faster than a player's CPU, so a schedule that follows the server's load alone fails against GPUs ([Green et al., LEET 2011](https://usenix.org/legacy/event/leet11/tech/full_papers/Green.pdf)).
- **Game servers.** Valve's servers answer a query from an address with a challenge before the data ([Steam, 2020](https://steamcommunity.com/discussions/forum/14/2974028351344359625/)), an address's proof, as QUIC's Retry, with no work.
- **Crates.** Arti's [`equix`](https://docs.rs/equix) and `hashx`, Rust Equi-X, are LGPL-3.0, where Campfire is MIT or Apache-2.0.

## Standards

- [RFC 9000](https://www.rfc-editor.org/rfc/rfc9000) — QUIC: the 1,200-byte minimum (§14), address validation and Retry (§8.1), connection migration (§9). An endpoint that wants to reset connections after it lost their state derives the reset tokens from a static key (§10.3).
- [RFC 9001](https://www.rfc-editor.org/rfc/rfc9001) — TLS for QUIC: TLS 1.3 only, ALPN required unless another mechanism agrees on the protocol (§8.1).
- [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) — the `tls-exporter` channel binding, for TLS 1.3, where `tls-unique` is undefined.
- [RFC 7250](https://www.rfc-editor.org/rfc/rfc7250), [RFC 8410](https://www.rfc-editor.org/rfc/rfc8410) — raw public keys in TLS; Ed25519 keys in X.509's structures.
- [RFC 6886](https://www.rfc-editor.org/rfc/rfc6886), [RFC 6887](https://www.rfc-editor.org/rfc/rfc6887) — NAT-PMP and PCP: a gateway's port mappings by UDP.
