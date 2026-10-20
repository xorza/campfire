# Transport

What a peer's bytes may cost a receiver, how the receiver ends a packet it refuses, and what a
send costs.

## Receive contract

- **Each channel has a maximum message size,** uncompressed, enforced on send with a typed error
  and on receive from the first fragment: a fragment count above the size, or one that implies
  more bytes than it, is refused, and decompression stops at it. The fragment count limit follows
  from it. Practice: GameNetworkingSockets' `RecvMaxMessageSize`, ENet's `maximumPacketSize`,
  yojimbo's `maxBlockSize`. Refused: a fixed protocol constant, which cannot fit every channel of
  every game; a per-channel maximum alone, which leaves 32,768 concurrent partials per reliable
  channel.
- **Each link's `Channels` has one receive budget,** validated at startup to admit the largest
  message plus a packet, and charged for reassembly reservations, retained final fragments and
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
- **A received packet ends one of three ways.** It is admitted, and accepted, so its packet session
  acknowledges it; refused when the budget is full, so it gets no ACK and its sender finds it lost
  and sends its reliable messages again (GameNetworkingSockets' "do not ack", ENet); or a protocol
  violation, which unlinks the peer with `UnlinkReason::ProtocolViolation`. A violation is a claim
  that no honest peer of the same protocol makes: bytes that do not decode, a payload kind or a
  tick no sender writes, an oversize or inconsistent fragment, an unknown channel or message id, a
  message against its registered direction, a decompressed size over the limit (renet, yojimbo,
  QUIC). No packet's outcome ends the pass for the packets behind it or for other links. Refused:
  drop-only, which keeps an attacker connected and loses reliable data already acknowledged.
- **Decoders take only what the bytes hold.** A capacity is bounded by what the reader has left,
  every narrowing of a peer value goes through `TryFrom`, and a peer's tick count reads as the
  unsigned delta it is.
- **A receiver learns each message's fragment size from its fragments.** The sender splits by its
  own packet session's largest payload, so the two peers' sizes may differ, as a client's packets
  carry a connection ID that a server's do not. Refused: one fragment size both peers must agree on,
  which no handshake checks.

## Protocol agreement

- **Both peers agree on the protocol before data flows.** The hashes cover the channels with
  their modes and settings, the components, the messages and the events, each by
  `core::any::type_name`, so peers must come from one build, which the check verifies. They are
  computed in `Plugin::finish` from the finished registries, and a mismatch unlinks with
  `UnlinkReason::ProtocolMismatch`. Refused: Bevy's `DebugName`, which hashes a constant unless
  `bevy_utils/debug` is on.
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
- **A reliable message goes again when the packet session finds its packet lost,** and waits while
  the packet is in flight, so it costs no timer of its own and no resend that an ACK would have made
  needless. Refused: a resend delay of a multiple of the RTT, which sends again before a late ACK
  and after a loss the packet session already found. Source: RFC 9002 §6.
- **A packet's tick goes in 2 bytes once the peer surely reads it whole,** and in 5 before: the
  sender writes the short form while the tick is within 2^14 of the largest tick of a packet the
  peer accepted and of the largest it sent, so the receiver's nearest tick to its own largest is
  the tick, as RFC 9000 §A.2 writes a packet number. Refused: a 4-byte tick in every packet; a
  short tick with no reference, which a receiver reads wrong after a gap.
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
- **A send candidate holds a key, and the packet builder reads the bytes from the channel,** so
  no `Bytes` is cloned for each candidate: the first clone of a `Vec`-backed `Bytes` allocates its
  shared header.
