# UDP

How `net-udp` makes a sealed packet session over a network, and `net-socket` carries it on the OS's
socket: its handshake, its trust, and how a server guards itself. The packet session that it seals is in [packet session](packet-session.md), its byte layouts in [the protocol spec](../05-protocol-spec.md#the-sealed-udp), and the facts behind it in [UDP research](../../research/udp.md).

## Handshake

- **The handshake is TLS 1.3 by rustls's QUIC API,** carried in our own handshake packets. The
  first key epoch goes in the clear, as TLS over TCP sends it, and the transcript hash protects
  it; the handshake epoch is sealed with the keys rustls gives. A flight goes again until the
  peer's next flight arrives, as DTLS 1.3 does, on a timer of 999 ms, the probe timeout of RFC
  9002's initial RTT of 333 ms, which doubles at each resend; the client sends its end again
  while the server sends its flight again. Each handshake packet carries a version byte and the
  client's 8-byte handshake ID. Refused: Noise, which has no WebPKI; DTLS, which has no complete
  Rust implementation; QUIC itself, whose ACKs and number space repeat the packet session's. Its
  cost: a third-party client implements the stack's own protocol from [the protocol
  spec](../05-protocol-spec.md), where any QUIC library connects. Source: RFC 9001 §4; RFC 9147
  §5.8.
- **The AEAD is ChaCha20-Poly1305 only,** with no key update. Its confidentiality limit is past the
  packets a packet session sends; a packet session ends at its integrity limit of 2^36 failed
  decryptions. Refused: AES-GCM, which needs a key update before 2^23 packets, 5 to 39 hours of
  play, a path that runs too seldom to stay tested. Source: RFC 9001 §6.6.
- **The MTU is a fixed 1,200 bytes.** Fragments and replicon's messages are sized from the
  minimum MTU, so a larger probed one helps only packets that carry many whole messages.
  Refused: RFC 8899 probes, a state machine for a small gain; a fixed 1,232, IPv6's guarantee
  with no margin for a smaller IPv4 path under a tunnel. Source: RFC 9000 §14.

## Trust

- **A server's identity and a client's trust match.** A server presents an Ed25519 raw public key
  (RFC 7250), which a client of `ServerTrust::pinned` takes and no other; or a certificate chain,
  which a client of `ServerTrust::web_pki` checks for its name against the roots of
  `webpki-roots`. TLS 1.3 only, the ALPN a required setting, no resumption and no 0-RTT, so each
  handshake checks the identity. Refused: a mode that skips verification, even in development,
  where a server makes a key and its client pins it; the OS's root store through
  `rustls-platform-verifier`, which needs a Java component on Android.
- **One server secret derives the raw key and the reset key,** by HKDF-SHA256 with the labels
  `campfire/tls/v1` and `campfire/udp-reset/v1`, so one file holds the identity and the pin
  cannot be lost while the identity stays. The TLS label is the one Campfire's QUIC transport
  took before the stack's UDP, so a server's key file keeps its published key. Refused: a new
  label, which changes each listing's pin once.
- **The app authenticates over the link.** No client key in TLS. Each end of a link holds RFC 9266's
  `tls-exporter` binding as `ChannelBinding`, which an app's login signs, so a party that relays a
  login holds two packet sessions with two bindings, and the relay fails. A plain packet session has
  none. Refused: client certificates, which put an account system in the transport.

## Admission

- **A server checks an address before it keeps state.** A Retry for each new address, whose
  token names the handshake; two tokens in the server's encrypted parameters that let the next
  connection from that address skip it within two minutes, each usable once; and a token key new
  at each start, so a token outlives no server. Refused: a Retry only under load, which lets each
  spoofed packet cost a signature and amplified bytes until the limit. Source: RFC 9000 §8.1.
- **A Retry checks each new address,** so the client pads each `Initial` to 1,200 bytes and the
  server drops a shorter one: no answer of the server to an address it has not checked is larger
  than what it received. Source: RFC 9000 §14.1.
- **Each address holds a bounded count of connections and handshakes.** `AdmissionSetup` holds
  at most so many live packet sessions for an IPv4 address or an IPv6 /64, and a bucket of as many
  handshakes that come back over its refill time, kept in one number as the generic cell rate
  algorithm keeps it. A refusal comes after the Retry and before the handshake, so it costs no
  signature. A packet session counts at the address it was admitted from, also after it moved. A
  loopback address holds no limit. An IPv6 /48, a site's delegation of 65,536 /64s, holds a bucket
  of 16 times an address's, so one site takes no more of a server than 16 hosts do, as IKEv2
  implementations limit by /48 or /64. Refused: the /64 alone, which a residential /48 multiplies
  65,536 times.
- **A hello spans `Initial`s.** A client sends its `ClientHello` in as many padded `Initial`s
  as it needs, each with its token, and sends them again together. The server's pending
  handshake starts at the first validated `Initial` of any offset, takes each later one of the
  handshake into its stream, and writes its flight once TLS read the whole hello, so a duplicate
  before then gets no answer. Each handshake is one enum of its phases on each side. Refused: a
  hello in one `Initial`, which a hybrid post-quantum share of 1,216 bytes is past. Source: quinn
  routes a pending connection's later `Initial`s to its `Incoming` (`quinn-proto` 0.11,
  `Endpoint::handle`).
- **A server bounds its pending handshakes.** It holds at most 1,024 pending at once, each a TLS
  connection for its patience, and refuses one past them before it starts, as quinn's
  `max_incoming` does; and it sends at most 1,024 stateless resets each second, each an HMAC for a
  datagram any address may make up, as RFC 9000 §10.3 lets an endpoint limit them. Refused: the
  bounds of each address alone, which addresses an attacker spreads over all pass.

## Packet sessions

- **A client packet starts with the server's random connection ID,** and a new address is checked
  before data goes to it. The server gives the ID, 8 bytes whose first bit is the IO's, in its
  handshake and routes each client datagram by it, so a packet session survives a NAT rebinding or a
  change of network. A newer packet from a new address makes it a candidate, which gets a path
  challenge until it answers; a challenge goes again at each probe timeout, 3 times at most, and a
  check that failed starts again at the peer's next newer packet. Refused: routing by address, as
  netcode.io does, which ends the link at each such change; a 15-bit ID, as an attacker collects
  the reset tokens of all 32,768 IDs from the server and resets each later packet session that takes
  one. Source: RFC 9000 §8.2, §9, §10.3.2 and §21.11.
- **A stateless reset ends the packet sessions of a server that started again.** Its token is an
  HMAC of the reset key and the connection ID, which the client gets in the handshake; the server
  answers a datagram of an ID it does not hold with a reset shorter than it, and none to a datagram
  of 21 bytes or less, so two endpoints never reset each other in a loop. Source: RFC 9000 §10.3.
- **A pending handshake holds its early datagrams.** A sealed datagram of a pending handshake's
  connection ID, which a link's jitter brings before the client's end, goes to that handshake,
  which holds a client's first frame of them for its packet session, and the reset answers only
  an ID no packet session and no handshake holds, as RFC 9001 §5.7 lets a server keep the 1-RTT
  packets that come before its handshake's end and RFC 9000 §5.2 resets only a packet no
  connection can claim.
  Refused: a reset for a pending ID, which ended the connection its handshake was about to make;
  a drop of every early datagram, which delays the session's first messages by a resend.
- **The socket stays on `quinn-udp`, with GSO and GRO.** A send joins the datagrams of one peer
  and one length; a read splits what GRO joined.
- **A wake sends one byte until a wait takes it.** The app wakes the IO thread's wait by a byte from
  one loopback datagram socket to another, which the poller watches, and sends it only while a flag
  says none is pending; the thread drains the byte, then clears the flag by an exchange that
  releases the drain and acquires each wake that sent nothing, so at most one byte is in flight and
  the pass after the wait reads what every wake brought, as libuv's async handle and `polling`'s
  `Poller::notify` coalesce their wakes, each clearing its flag by a read-modify-write. Refused: a
  byte for each wake, which a macOS loopback datagram still in flight after its `send` lets outlive
  a wait's drain and end the next wait for nothing; `Poller::notify`, which allocates on each call
  on kqueue. Source: [UDP research](../../research/udp.md#a-wake-in-flight).
- **A wake that fails ends the process.** A wake's send that would block leaves its wake pending,
  as the full buffer holds a byte the thread has yet to take; an interrupted send sends again; any
  other failure panics, and only a wait's drain clears the flag, as tokio's I/O driver panics on a
  failed wake and libuv's async handle aborts. Refused: the flag cleared after a failure, which
  loses a wake that came during the send and found the flag set, so an idle wait of no timeout
  stalls; the flag kept set after a failure, as `polling`'s `Poller::notify` keeps it, which stalls
  the thread the same way. Source: [UDP research](../../research/udp.md#a-wake-that-fails).
