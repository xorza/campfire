# Transport

What a peer's bytes may cost a receiver, how the receiver ends a packet it refuses, and what a
send costs.

## Receive contract

- **Each channel has a maximum message size,** enforced on send with a typed error and on receive
  from the first fragment: a fragment count above the size, or one that implies more bytes than
  it, is refused. The fragment count limit follows from it, and a link whose fragments cannot
  carry it fails at its link. Practice: GameNetworkingSockets' `RecvMaxMessageSize`, ENet's `maximumPacketSize`,
  yojimbo's `maxBlockSize`. Refused: a fixed protocol constant, which cannot fit every channel of
  every game; a per-channel maximum alone, which leaves 32,768 concurrent partials per reliable
  channel.
- **Each link's `Channels` has one receive budget,** validated at the channel registry's seal to
  admit each channel's largest message plus a fragment, and charged for reassembly reservations, retained final fragments and
  completed messages that wait behind an ordering gap. Practice: ENet's `maximumWaitingData`,
  GameNetworkingSockets' `RecvBufferSize`, QUIC's connection window. Refused: renet's per-channel
  budget, which multiplies by the channel count.
- **A channel opens to a link the app admits.** A channel registered with `add_direction_after::<G>`
  gives a link of its receiving side its receiver only once the link holds `G`, so a peer the app
  has not admitted, as a client that did not join, fills no such channel, and a packet on it is a
  protocol violation; the receiver stays for the link's life, so a message in flight is not refused.
  Refused: a receiver on every link from its link's start, which lets a peer before its admission
  fill the channel to the receive budget; a check in the app's systems, which runs after the bytes
  were held.
- **A channel has one reader.** The message layer reads each channel by the message types its
  bytes name, but one registered with `read_by_owner`, which its own layer reads alone and drains on
  a link it no longer applies it to, as replication drains replicon's channels on a client with no
  `ReplicationReceiver`. Refused: the message layer reading every channel, which decoded a
  replication message as whatever message its first bytes named once its layer stopped reading.
- **A received packet ends one of three ways.** It is admitted, and accepted, so its packet session
  acknowledges it; refused when the budget is full, so it gets no ACK and its sender finds it lost
  and sends its reliable messages again (GameNetworkingSockets' "do not ack", ENet); or a protocol
  violation, which unlinks the peer with `UnlinkReason::ProtocolViolation`. A violation is a claim
  that no honest peer of the same protocol makes: bytes that do not decode, a payload kind or a
  tick no sender writes, an oversize, empty or inconsistent fragment, a batch of no message, an
  unknown channel or message id, a message against its registered direction or of no declared
  direction (renet, yojimbo, QUIC). A refusal ends no pass, and a violation ends its own link's
  pass, as the link is unlinked, and no other link's. Refused:
  drop-only, which keeps an attacker connected and loses reliable data already acknowledged.
- **Decoders take only what the bytes hold.** A capacity is bounded by what the reader has left,
  every narrowing of a peer value goes through `TryFrom`, and a peer's tick count reads as the
  unsigned delta it is.
- **A receiver learns each message's fragment size from its fragments.** The sender splits by its
  own packet session's largest payload, so the two peers' sizes may differ, as a client's packets
  carry a connection ID that a server's do not. Refused: one fragment size both peers must agree on,
  which no handshake checks.

- **What a channel holds past a packet is a copy, charged with its entry.** A message behind a gap
  and a final fragment that waits for its size are copied out of their packet, which the receive
  arena's 256 KiB chunk holds, and each entry costs its slot as well as its bytes, so a message
  16,383 ids ahead costs one entry, not a slot for each id between, and empty messages fill the
  budget too. The entries are a B-tree by id, so a peer's order of ids costs `O(log n)` each.
  Refused: a slice of the packet, which pins its chunk; a ring at each id's distance; a list
  sorted by distance, which moves half its entries for each id a peer sends into its middle.
- **An id past the sender's window breaks the protocol.** A reliable message as far as
  `SEND_WINDOW` or farther ahead of the id its receiver waits for unlinks its peer, as the
  sender's window of unacked ids starts at or before that id, so no honest peer sends one.
  Source: QUIC, whose receiver closes on data past the flow control limit it gave (RFC 9000
  §4.1).

## Protocol agreement

- **Both peers agree on the protocol before data flows.** The hashes cover the channels with
  their modes and settings, the components, the messages and the events, each by
  `core::any::type_name`, so peers must come from one build, which the check verifies. They are
  computed in `Plugin::finish` from the finished registries, and a mismatch unlinks with
  `UnlinkReason::ProtocolMismatch`. Refused: Bevy's `DebugName`, which hashes a constant unless
  `bevy_utils/debug` is on.
- **The hashes travel in the ALPN, planned.** A UDP link's ALPN names the app and the hash of its
  three registries, so a build that differs fails the TLS handshake with `no_application_protocol`,
  which unlinks with `ProtocolMismatch`, before a packet session exists; a plain link comes from
  one build and checks nothing. Source: QUIC, which agrees on its application protocol by ALPN and
  closes the handshake on none, with a name for each version of a protocol (RFC 9001 §8.1); Unity
  Netcode for Entities, which checks its RPC and component hashes in its handshake. Refused: an
  event of the registries it checks, whose own ids shift in a build that differs and which the
  server sends beside the first data; the hashes in the transport parameters, a refusal path of
  its own that hides no more; a reserved channel sent first, a checked state that every data path
  waits on.
- **Replicon's channels map from `RepliconChannels`,** one the stack channel for each replicon
  channel, each with one direction.

## Send path

- **A link's `Channels` queue its sends in a `Vec`, in the `Transport` component,** drained with
  its capacity kept, and a producer takes `&mut Transport`, as replicon's `ResMut<ServerMessages>`
  and renet do.
  Refused: a bounded channel, which drops reliable messages or deadlocks; a `Mutex<Vec>`.
- **A flush stops at the packet session's budget,** the smaller of its congestion window's room and
  the app's rate, so congestion holds back the lowest priorities, which the flush orders by priority
  each time. Refused: the IO dropping what it cannot send, by age and not by priority; an order only
  under a rate limit, as the budget always holds.
- **A link's queued bytes have a budget,** 1 MiB, twice the default largest message as the
  receive budget is: its staged messages and what its senders hold until they send them, or until
  their acknowledgement on a reliable channel. A send past it, or past its channel's largest
  message, is dropped from an unreliable channel, whose messages may go missing, and on a reliable
  one ends the link, `Backlogged` or `MessageTooLarge`, which sends nothing more: a dropped reliable
  message would leave its peer a stream with a hole for good, and a slow peer that still
  acknowledges never reaches the idle timeout. Practice: renet disconnects at
  `ReliableChannelMaxMemoryReached`; GameNetworkingSockets' `SendBufferSize` returns
  `LimitExceeded`. Refused: a refusal for each caller to answer, which every caller logged and
  dropped.
- **A sequenced unreliable channel sends its newest message,** and drops each older one no packet
  carried, as its receiver takes only the newest; a fragmented message that started keeps its
  fragments. Refused: a send of every queued version, which spends the budget on what the
  receiver drops.
- **A reliable message goes again when the packet session finds its packet lost,** and waits while
  the packet is in flight, so it costs no timer of its own and no resend that an ACK would have made
  needless. Refused: a resend delay of a multiple of the RTT, which sends again before a late ACK
  and after a loss the packet session already found. Source: RFC 9002 §6.
- **A packet's tick goes in 2 bytes once the peer surely reads it whole,** and in 5 before: the
  sender writes the short form while the tick is within 2^14 of the largest tick of a packet the
  peer accepted and of the largest it sent, so the receiver's nearest tick to its own largest is
  the tick, as RFC 9000 §A.2 writes a packet number. One byte of the sender's overstep follows it
  ([Time](time.md), A packet carries its sender's instant). Refused: a 4-byte tick in every
  packet; a short tick with no reference, which a receiver reads wrong after a gap.
- **A queued message keeps its channel's type until the flush,** which maps it to the channel's
  id there, at the type boundary where the registries keep their one map. A sender is a
  component, with no registry when it queues. Refused: senders that take a channel id, which
  break every `send::<C>`; one channel for each message type, bound at registration as
  bevy_replicon binds it, which takes away sending one type on several channels; a cache of the
  last channel, a branch to spare one lookup.
- **A network target holds its peers in one sorted, shared list,** so a set has one form, a clone
  shares it, and a lookup is a binary search. The resolver scans a small target and hashes a
  large one, as the connected clients come in link order. Refused: a merge of sorted lists,
  which needs a second index of the clients in peer order, kept on each connect and disconnect.
- **A message's id goes with no tag.** The channel's mode tells whether one follows, and both
  peers register the same modes.
- **An idle transport sends its tick.** A flush with no message, 100 ms or more after the last
  packet, sends a packet of the tick alone. The peer's timeline reads the tick of each packet,
  and the link's RTT samples come from its ACK, so an idle peer would give neither. Refused: ping
  messages, which cost bytes on a busy link too; the packet session's keep-alive, which carries no
  tick.
- **Packets go uncompressed.** Refused: LZ4 of each packet and of each fragmented message, which
  no app turned on, whose setup no protocol hash held, and whose fit search took most of the packet
  builder; a game whose bytes compress well compresses them in its own messages.
- **A packet holds at least its first candidate.** A fragment fits a packet by the fragment size,
  and a whole message is at most a fragment long, so staging has no failure: a packet past the
  payload is a bug the builder asserts. Refused: a staging error that ended the flush, which left
  the link connected and sending only its tick.
- **A send candidate holds a key, and the packet builder reads the bytes from the channel,** so
  no `Bytes` is cloned for each candidate: the first clone of a `Vec`-backed `Bytes` allocates its
  shared header.
