# Packet session

How `net-packets` gives each link one numbered, acknowledged and, on a network, sealed
packet stream under the transport's messages. The sealed handshake is in [UDP](udp.md).

## Layering

- **Per-packet state is the packet session's, per-message state the transport's.** The nonce, the
  ACKs, loss detection, the RTT samples, the bytes in flight and the replay window all use the
  packet number, so one holder removes a second number and a second ACK layer. Refused: a
  crypto-only layer, which must number packets again for its nonce, as netcode.io and reliable.io do
  in yojimbo; channels in the packet session, which would only move the transport's tested message
  logic; QUIC under the stack, whose ACKs and RTT repeat the stack's. Source: RFC 9001 §5.3;
  GameNetworkingSockets, which sends again from the result of each packet.
- **The seam is a set of queues on `Link`.** The frame puts each payload with its kind and a token
  of its own, and each packet it accepted; the packet session puts each opened payload with its
  number, each payload's fate by token, its RTT samples, its budget and its largest payload. The
  transport never sees a packet number before it is sent, and the packet session never calls the
  transport. The queues and their types are `net-payload`'s, so the transport and the packet session
  each depend on it and neither on the other. Refused: calls from the transport into the packet
  session, which need its lock in each transport system.
- **An ACK means that the transport accepted the packet.** Only an accepted packet gets an ACK,
  so a drop before the accept, in the kernel, in a queue or in the transport's receive budget, is
  always safe: the sender sends again. Refused: ACKs at reception, as QUIC sends them, which turn
  each later drop into a lost reliable message. Source: WebTransport issue #23.
- **Every link runs the packet session, and the IO decides whether it is sealed.** A socket takes
  only the handshake's seal; the emulated network takes either; a later IO that encrypts itself, as
  WebTransport or Steam's relay, takes a plain one. Refused: always sealed, which gives every
  in-process test an identity and a later WebTransport link two layers of encryption; in-process
  links with no packet session, which gives the transport two sources of packet results.
- **Two IO kinds: the socket and the emulated network.** The emulated network on the OS clock
  with a perfect model is the in-process link. Refused: crossbeam as a third IO with the same
  behaviour.
- **One endpoint core and one pass for each IO.** An endpoint holds its packet sessions and its
  establishment behind one mutex, its core, and its IO beside it, which the IO thread reads, writes
  and waits on outside the lock; its pass takes the IO's datagrams, opens them,
  runs the timers and sends. On the OS clock its IO thread runs the pass when a datagram may have
  come, a timer is due or the frame wakes it, so a frame that stalls stops no timer and no
  keep-alive; on a driven clock the frame runs the same pass itself, so a seed gives the same run.
  The frame takes the lock in the endpoint's two systems alone. Refused: a thread in lockstep on a
  driven clock, which adds two handoffs a frame and runs nothing in parallel; the frame as the only
  driver, which a stall stops.
- **The IO thread wakes the frame.** A pass that opens a payload for the host, or leaves it news,
  raises the app's `FrameWake`, which every endpoint of the app shares, so a host that waits on it
  between its ticks reads a link's work as it comes, and runs no frame for a keep-alive or an ACK.
  Practice: Source's dedicated server, which sleeps on its sockets until its next tick or a packet.
  Refused: a frame on a fixed short loop, which runs every system for nothing between ticks; a
  wake from each datagram, which an idle link's keep-alives would raise.
- **A pass touches only the packet sessions that need it.** A datagram finds its packet session by
  its route in a map, and a pass polls the packet sessions whose timer is due in a heap of
  deadlines, so a pass costs what its datagrams and due timers cost, whatever the endpoint holds.
  Refused: a walk of every packet session for each datagram and each pass, which a server of a
  thousand clients at 60 Hz runs tens of millions of times a second.
- **A pass opens a bounded batch under each hold of the lock.** A hold takes at most a batch of
  work, datagrams and due packet sessions together, so a burst or a sweep of timers holds the
  frame's endpoint systems for one batch: 68 µs as 4,096 keep-alives come due, against 1.34 ms
  unbounded ([Endpoint holds](../../research/endpoint-holds.md)). Refused: a read until the socket
  is empty; quinn's time budget, whose boundaries differ in each run of a driven test. Source: quinn
  bounds its endpoint loop at 160 datagrams, the lowest that kept its throughput.
- **A hold starts at most one handshake,** which `Establish::receive` reports with no default, as a
  TLS server's first flight costs 58 µs ([Endpoint holds](../../research/endpoint-holds.md)); quinn
  starts each accepted connection under a hold of its own. Refused: a smaller batch, which slows
  every burst for a cost only a start has.
- **The IO thread wakes only for a deadline it does not know.** The frame's send wakes it only when
  the core's next deadline is earlier than the one it sleeps to, as quinn moves its timer only when
  `poll_timeout` changes. Refused: a flag set when the frame took a payload, which wakes for
  datagrams already sent.
- **The IO thread sends, planned.** On the OS clock the frame seals each link's packets under the
  lock, queues them, and wakes the IO thread, which makes the sends, so the frame pays one wake
  however many peers it sends to; a driven clock sends in the frame's pass as now. This replaces
  the wake only for an unknown deadline for a frame that queues datagrams. Source: quinn, which
  sends from its endpoint driver, and `quinn-udp`, which sends one destination a call, so a
  server's tick of one packet per client gets no GSO batch. Refused: a send per peer on the frame
  thread, which grows each frame with its players, 1.5 to 3 ms of a 16.7 ms tick at a thousand
  clients; `sendmmsg` across peers, `unsafe` of our own on Linux alone, before a measurement shows
  the IO thread is the limit.
- **A path check is a server's capability,** which both routes by connection ID and challenges a new
  path, as only the server routes so ([UDP](udp.md)). Refused: a `bool` beside a challenge method,
  which leaves a client methods it cannot reach.
- **The link conditioner goes into the endpoint, before the packet sessions,** so their RTT samples,
  loss detection and window see the delay and loss it adds. Refused: the conditioner on the frame's
  queues, whose delay the RTT samples would leave out.
- **The endpoint is generic over its IO, and the establishment and the seal are trait objects,**
  each chosen for each endpoint at run time, so the packet session does not depend on rustls.
- **The endpoint core names each packet session by its carrier,** a type of its host's: the ECS
  gives its `Entity`, a test a number. Refused: a map from each packet session to its entity in the
  ECS crate, which keeps an entry for each packet session its own end closed.
- **An IO crate gives the parts of an endpoint,** its IO, its establishment, its side and its
  setup, and `net-endpoint` makes the component from them, so no IO crate depends on the
  ECS crate. Refused: an IO crate that builds the component, which puts the engine under it.
- **One generic `EndpointPlugin<Io>` registers each IO's endpoints,** in `net-endpoint`.
  Refused: a plugin for each IO, which only wraps the generic one.
- **A server's endpoint spawns each link, and links it in a later command,** with its packet
  session's budget and largest payload already handed over: an app's observer of the spawn adds its
  components before `Linked`, as a test's stack does, and the transport sizes its fragments at
  `Linked`. Refused: an app that spawns the links, which cannot know a peer before its handshake.
- **A start of an endpoint that is linked or linking keeps its packet sessions,** so a second
  `Connect` is no reconnect. Refused: a new packet session at each start, which drops a working
  link.
- **An IO copies each datagram it reads into an arena of four chunks,** the socket and the
  emulated network alike: a read's datagrams share one allocation, which a message slice of the
  frame never promotes, so a sealed frame allocates nothing. Refused: one buffer for each
  datagram; the emulated wire's own bytes, which the sender's payload arena shares.
- **One `ByteArena` type, in `net-bytes`, holds every buffer that views split off.** These are the
  writer's messages, the payloads of a flush, a packet session's control packets and an IO's
  datagrams. Each fills a chunk from the front, reuses it once its views drop, and keeps a bound of
  spare chunks. Refused: four versions of the same reclaim, each with its own retention rule.

## Wire

- **The header is the AEAD's associated data: authenticated, not encrypted.** Refused: QUIC's
  header protection, which only hides the packet number from middleboxes. Source: the QUIC
  overhead of 36 bytes for each packet, measured under `quinn-proto` 0.11.19 on the emulated
  network.
- **A header decodes from one encoding.** A `ToBytes` decode refuses an `Option` tag other than 0
  or 1 and a varint longer than its value needs, as every format of the engine keeps one encoding
  of each value (02-engine-core.md, One encoding of each value). Refused: two spellings of one
  header, which a third party's implementation of the protocol would meet.
- **The first bit of each datagram is the IO's,** for its handshake, Retry and reset packets.
- **A packet number goes as its low 16 bits,** which the receiver makes whole against the largest
  it opened, as RFC 9000 §A.3 does; a sender sends nothing past the numbers its peer reads whole.
- **One window of 64 packets is the ACK bits, the replay window and a frame's payloads.** An ACK
  names its largest and the 64 below it, a frame holds the payloads of the 64 numbers at and below
  the largest opened, so one ACK names every packet one frame accepts. A packet older than the
  window is dropped, as its replay cannot be told; a replay of a packet that was refused delivers
  genuine data late, as a network delay does. Refused: 32 bits, as Gaffer on Games' scheme for a
  packet a frame each way, under which a burst of more than 33 packets in one receiver frame lost
  its oldest for want of a bit, sent them again and halved the congestion window; ACK ranges, as
  QUIC and GameNetworkingSockets send them, more code for bursts the frame already bounds at 64.
  Source: RFC 4303 §3.4.3, which prefers a window of 64.
- **The packet session carries 2 bits of its payload's kind, which it does not read,** as IP's
  protocol field names the layer above.
- **Each ACK says how long its largest waited,** in a byte of quarter milliseconds, so each ACK
  gives an exact RTT sample and no link needs pings; a wait the byte cannot hold, 63.75 ms or more,
  gives none, as the sample would be the wait's own length.

## Recovery and rate

- **The RTT comes from the ACKs.** The packet session keeps RFC 9002's estimate for its timers; a
  reader outside it takes each sample less the peer's whole ACK delay. Source: RFC 9002 §5.
- **Loss is found by the packet threshold of 3, or by 9/8 of the RTT,** and the probe timeout
  sends a `Ping`, whose ACK lets loss detection find what was lost. Refused: sending data again at
  the timeout, which saves a round trip only when the lost packet was the last one sent. Source:
  RFC 9002 §6.
- **An ACK-only packet goes when an accepted packet waited 25 ms and no packet carried its ACK.**
  The delay is a protocol constant, so no link negotiates it; and one goes at the end of a frame
  whose accepted packets no ACK named pass half the window, so a second such frame leaves none
  unnamed, as QUIC acknowledges every second packet. Source: RFC 9000 §13.2.1, §13.2.2.
- **Congestion control is NewReno, in the packet session.** The budget is the smaller of the
  window's room and the app's `BandwidthLimit`, so one budget exists; the initial window is 10
  packets, 12,000 bytes. Refused: Cubic or BBR, more code for app-limited traffic; pacing, which a
  tick's burst does not need; the transport's own rate limiter, a second budget. Source: RFC 9002
  §7.
- **The keep-alive and the idle timeout are the packet session's,** on every IO, and only an
  authenticated packet resets the idle timer.
- **A close carries its code,** and a closed packet session answers each packet of the next 3 probe
  timeouts with its close again; the peer's own close ends that at once. Source: RFC 9000 §10.2.
