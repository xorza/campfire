# Campfire — Protocol Spec

What a third party needs to read and verify session logs, and to build a storefront, a server browser or a wallet integration. This document is a specification: its byte layouts are the protocol, and each changes only with its tag.

## Scope and versioning

- **The protocol is apart from replication.** State replication between client and server (`net`) is internal, as both run the same engine release; a client and a replay come from the release the log names. The protocol has its own version, which every log and event states.

## Encoding and cryptography

| Item | Rule |
| --- | --- |
| Binary encoding | `postcard` 1.x |
| Numbers in game data | 40.24 fixed point, as raw `i64` |
| Anything published as a file: package files and fingerprints, session logs, snapshots | SHA-256, 32 bytes, so [Blossom](https://github.com/hzrd149/blossom) servers host it as is |
| State hashes, and every other hash the protocol and the sim make | BLAKE3, 32 bytes, in its key derivation mode with a domain tag `campfire/<name>/v<n>` as the context: `derive_key(tag, input)` |
| Signatures | Schnorr over secp256k1, as in Nostr |
| Nostr events | Standard Nostr JSON; payloads defined here |

- **Hashes, ids and keys.** A name says what a 32-byte value is: a `ContentHash` is the SHA-256 of a file; a `…Fingerprint` the content hash of an artifact published whole; a `…Hash` a BLAKE3 digest of a domain; an `…Id` an identity made by a hash; a `…Commitment` a digest that binds a secret before it shows. Hex is 64 lowercase digits, the one spelling a reader takes.
- **A hash's domain is BLAKE3's context.** A domain's tag is the context of BLAKE3's key derivation mode, which hashes it apart from the input, so no input extends one tag into another. The tag as the input's first bytes is refused, as `campfire/a/v1` before `0` reads as `campfire/a/v10`; a length prefix is refused, a hand-made encoding of what the mode gives ([BLAKE3](https://docs.rs/blake3/latest/blake3/fn.derive_key.html), `derive_key`).

## Packages

- **A package is its index.** `package.index` lists one row per other file, sorted by path bytes: the line `campfire/package-index/v2`, then `<SHA-256> <size> <path>` per file, fields one space apart, each line ending in LF. It is text, as Go's module hash and BagIt's manifests list a tree's files, so a reader and a diff show it. Its fingerprint is the SHA-256 of the index's bytes. A path is names joined by `/`, none empty, `.` or `..`; an index of two paths that differ only in case, a file past 2 GiB, a path out of order, more than 64 MiB, or any other encoding of its rows does not read, so one package has one fingerprint.
- **Reads on demand.** A load reads the index, then only the files the session needs, each checked against its row; a file the index does not list is no part of the package, so a package costs what its session reads while the fingerprint names every byte, as a git tree names its blobs.
- **One blob per file.** An update downloads only changed files, and packages share identical assets.
- **A package holds each SHA-256 once.** Each row reads from the first path of its SHA-256, in the index's order, that the package holds by its exact name, and is missing only when it holds none, as a git tree names one blob for every path of the same bytes; the writer writes only the first path it is given of each SHA-256. Refused: a hard link or a copy at each other path, which `cp -r`, `robocopy` and an archiver turn into full copies and FAT into a copy at the write; a table of aliases beside the index, which holds what two rows of one SHA-256 already say.
- **Imported packages are made, not fetched.** Each machine makes an imported package with the `import` app from its own copy, the same bytes on every OS; a listing names it by fingerprint with its import recipe: the importer release, the game and its version ([Zero Hour](09-zero-hour.md)).

## Keys

| Key | Owner | Signs |
| --- | --- | --- |
| Main key | Player (Nostr identity; ideally a remote signer, or a local file encrypted with [NIP-49](https://github.com/nostr-protocol/nips/blob/master/49.md)) | Session key delegations, licenses held, reputation statements |
| Session key | Player's client, one per session, short-lived | Chain heads of the player's inputs |
| Server key | Host (Nostr identity) | Listings, checkpoints, bot and external inputs, receipts, results |
| Author key | Content creator | Package announcements, license key delegations |
| License key | A storefront the author delegates to | Licenses |
| Group key | A group of hosts | Membership, shared ban lists |
| Release keys | The maintainers, offline; a launcher pins the set and its threshold k of n | Releases, revocations, key-set changes |

- **A signature's randomness comes from a seeded draw.** A signer fills a secret seed from the OS once, and draws each signature's auxiliary randomness from it by BLAKE3 of the seed and a count, a pseudorandom function in counter mode, so a signature makes no system call; BIP-340's nonce hashes the key and the message too, so a draw two signatures share stays sound. Refused: 32 bytes from the OS for each signature, a system call on the tick's thread for each.

- **Session id.** The hash of the session's terms, fixed before any player joins: `BLAKE3("campfire/session-id/v6" ‖ server key ‖ u32 tick rate ‖ u64 max input lead ‖ u32 max payload length ‖ u32 max inputs per tick ‖ seed commitment ‖ u64 release tag length ‖ release tag ‖ mode fingerprint ‖ u64 map name length ‖ map name ‖ u64 dependency count ‖ dependency fingerprints ‖ u64 player_slot count ‖ u8 player_slot plan per player_slot ‖ u64 visibility length ‖ visibility ‖ u64 option count ‖ u8 per option ‖ u64 player_slot pick count ‖ u8 per player_slot pick)`, integers little-endian, a player slot's plan 0 for a player, 1 for a bot, 2 for open. The visibility is the settings of [Replication audiences](10-replication-audiences.md): a `u8` for each kind a mode may set, its recipients beside the owner as bits (allies 1, enemies 2, spectators 4); the spectators' fog (0 `all`, 1 `teams`); what a player out of play sees (0 `team`, 1 `spectators`); and the most spectators as a `u32`. An option's byte is the place of the value the host picked among its values, the mode's options in the order of their names ([Sessions](08-sessions.md#decisions)); a player slot pick's byte is the place of the value the host picked of a choice of listed values for a player slot, 255 for none, player slot by player slot, each player slot's choices in the order of their names. Every delegation and chain-head signature names the id, so the players sign the terms and a log cannot change them. The seed commitment is fresh for every session, so no two share an id.
- **Session key delegation.** A Nostr event of kind 22710, signed by the main key, as remote signers (NIP-46) sign only events; the ephemeral range keeps a relay sent one by mistake from keeping it. It is never published: the log header holds it verbatim. Each term is one tag of one value: `session_key`, `server_key`, `session_id` and `seed_contribution` in lowercase hex, and NIP-40's `expiration`. Its JSON holds at most 4,096 bytes in the one spelling nostr writes, and a reader refuses any other, as no signature covers the spelling. The contribution is the player's random share of every segment's seed, chosen after the server's seed commitment is fixed, so no one can steer it.
- **License key delegation.** The same shape, signed by the author key, with the package ids it may license and an expiry, so a storefront issues licenses while the author is offline.

## Connection

- **The connection binds identities to the transport.** The game traffic inside it is not the protocol's.

1. The server presents a raw Ed25519 key to TLS, with no certificate, derived from its own key ([UDP](11-net-stack/udp.md#trust)); the TLS key in its signed listing proves the client reached the server its key names.
2. The client connects over the stack's sealed UDP, below, and takes only the TLS key it pinned; a server answers a new address with a Retry first.
3. The server offers the session's terms, its grace period and restore window, and a random challenge, for at most 10 s; a link still silent then closes with `Unanswered`, so a connection that only keeps itself alive holds no link. The client checks the terms against the listing and its own release and packages, delegates a fresh session key, and replies with a session-key signature over `"campfire/connect/v2" ‖ challenge ‖ channel binding`, the `tls-exporter` value of [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266), so a reply relayed from another TLS session is useless. A refused join is told by a word on the reliable channel and the close code `JoinRefused`. Players take player slots in the order their replies are accepted, one player slot for each main key; the newest login wins its player slot ([Sessions](08-sessions.md)). A spectator answers as a player does, holds no player slot, and the log holds nothing of it.
4. A home server asks its router to forward its port, by PCP, NAT-PMP or UPnP-IGD; LAN servers announce their signed listing over mDNS.
5. Against floods, cheap checks come first: the Retry, limits per address, and a packet's size before its signature.

### The sealed UDP

Each integer is big-endian. A datagram's first bit is 1 on a handshake packet and 0 on a packet session's, and each field has one encoding: a reader drops a datagram that holds any other.

- **TLS.** TLS 1.3 with RFC 9001's key schedule, as QUIC version 1 derives its packet keys, and the one cipher suite `TLS_CHACHA20_POLY1305_SHA256`; the ALPN `campfire`; a pinned client's server name `campfire`; no resumption and no 0-RTT. TLS's `quic_transport_parameters` extension carries the client's version, the byte 1, and the server's 91 bytes: the version 1, the packet session's 8-byte connection ID, its 16-byte reset token, and two 33-byte address tokens for the client's next connections.
- **Handshake packets.** A header of 10 bytes: the type, `0x80 | type << 4`; the version, 1; and the 8-byte handshake ID the client draws at random, which every packet of its handshake repeats. The first key epoch goes in `Initial` packets in the clear, the handshake epoch in `Handshake` packets; a crypto frame is a `u32` offset in its epoch's stream, a `u16` length and the data. A flight goes again until the peer's next flight arrives, first after 999 ms, then at twice the last wait.

| Type | Packet | Body after the header |
| --- | --- | --- |
| 0 | `Initial` | `u8` 0, or 1 and an address token of 33 bytes; a crypto frame. A client's hello takes as many `Initial`s as it needs, each with the token; a client pads each of its `Initial`s with zeros to 1,200 bytes, and a server drops a shorter one |
| 1 | `Retry` | The address token of 33 bytes that the client's next `Initial` carries |
| 2 | `Handshake` | A `u16` packet number; a crypto frame sealed with the handshake epoch's key, the number its nonce, the header and the number its associated data, its 16-byte tag last |
| 3 | `Refuse` | None: the server takes no handshake from the address now |
| 4 | `Alert` | The TLS alert, one byte |

- **An address token is the server's.** A client keeps the 33 bytes as it got them and reads none of them.
- **Packet session packets.** A client's datagram starts with the connection ID, whose first bit is 0; a server's starts with the header. The header is 3 bytes, or 14 with acknowledgements: the flags; the low 16 bits of the packet number; then, with acknowledgements, the low 16 bits of the largest number accepted, a `u64` of a bit for each of the 64 numbers below it, the nearest in the lowest bit, and a `u8` of the largest's ACK delay in units of 0.25 ms. The flags hold, from the high bit: 0; the type in 3 bits; the payload's kind in 2 bits, 0 on every type but `Payload`; whether acknowledgements follow, on each `AckOnly` and on a `Payload` only; and a reserved 0.

| Type | Packet | Body |
| --- | --- | --- |
| 0 | `Payload` | The game traffic, of the kind the flags name |
| 1 | `AckOnly` | None |
| 2 | `Ping` | None: the keep-alive and the probe of a timeout |
| 3 | `PathChallenge` | 8 random bytes, which a new path echoes |
| 4 | `PathResponse` | The 8 bytes of the challenge |
| 5 | `Close` | The `u64` code of the close: 0 ended with no reason, 1 dropped, as the app exited; Campfire's from 16: `Left`, `Superseded`, `InputRefused`, `ChainRewritten`, `SessionRefused`, `Unanswered`, `JoinRefused` |

- **The seal.** Each packet session packet is sealed with ChaCha20-Poly1305 under the TLS handshake's 1-RTT keys, each direction its own: the nonce from the whole packet number, which a reader expands from its low 16 bits as RFC 9000's appendix A does; the header, not the connection ID, as the associated data; the 16-byte tag after the body.
- **A stateless reset.** A server that holds no packet session of a datagram's connection ID answers with at most 64 bytes, shorter than that datagram and none to one of 21 bytes or less: a first bit of 0, random bytes, and the reset token of the ID last. A client that receives a datagram of at least 21 bytes whose last 16 are its packet session's reset token ends that packet session.

## Session log

```
SessionLog
  header
    protocol version
    terms: server pubkey, tick rate, max input lead (ticks),
           max payload length, max inputs per player per tick, seed commitment,
           engine release tag, mode package fingerprint, map name, dependency fingerprints,
           player slot plan: player, bot or open per player slot, visibility settings, option picks
           (the session id is their hash, and is not written)
    player slots: each player slot's start: a player's session key delegation, bot or open
  first segment: 0, or the one whose checkpoint the file starts at
  segments[] from the first
    checkpoint (every segment but segment 0): segment, tick, state hash,
                snapshot fingerprint, log carry, server signature
    ticks[]: entries[] logged before the tick ran, each one of:
             player packet: player slot, inputs[]: stamp tick, payload,
                            session-key signature over the chain head after the last input
             server input: the input, server signature at its tick and index
  entries[] logged after the last tick
  seed reveal: server seed of the last segment, which reveals every earlier one
  result (optional): tick, outcome, final state hash, server signature
```

- **File.** A log file starts with the tag `campfire/session-log/v13`, then postcard: the header; the `u32` number of its first segment, 0 for a file of the whole session, or a later one for a file a host kept from that segment's checkpoint on; the `u32` count of its segments, each its checkpoint record and 64-byte signature but segment 0; the `u64` count of its ticks and the entries logged before each; the entries after the last tick; the seed reveal and the result as options. Entries go as a `u32` count, each as its kind's `u8`: a player packet, 0, as `u32 player_slot`, its inputs as a `u32` count of `u64 stamp tick, payload bytes`, and the 64-byte signature; a server input, 1, as the input and its signature. A reader accepts only the canonical encoding, so one log has one fingerprint, and refuses every older tag.
- **Checkpoints and results.** A checkpoint record is `{u32 segment, u64 tick, state hash, snapshot fingerprint, log carry}`, signed over `"campfire/checkpoint/v1" ‖ session id ‖ postcard of the record`. The log carry is the log's own state at the boundary, so a file that starts at the checkpoint verifies from it alone, the snapshot's fingerprint and state hash checked against the record before the replay. The result is `{u64 tick, outcome, final state hash}`, the outcome `won` with a team's index, `draw` or `aborted`, signed over `"campfire/result/v1" ‖ session id ‖ postcard of the result`.

### Input sources

| Source | Signed by | Payload |
| --- | --- | --- |
| Player | Session key | Commands, each in its capability's format |
| Bot | Server key | A player input's format |
| External | Server key | Connect or disconnect, carry load, admin command, payment event, calendar time |

- **Segments.** A match starts one segment at tick 0; a world adds a checkpoint every few minutes, a save adds one, and a load starts a new segment from a save's checkpoint ([Sessions](08-sessions.md)).
- **Seed chain.** At the session's opening the server fixes every segment's server seed as a one-way hash chain, the scheme of Lamport's one-time passwords: `s_k = C^(N−1−k)(r)` with `C(x) = BLAKE3("campfire/seed-chain/v1" ‖ x)`, so each seed is the hash of the next. The terms commit to `C(s_0)`, and revealing `s_k` reveals every earlier seed and no later one.
- **Seed.** Segment `k`'s seed is `BLAKE3("campfire/segment-seed/v1" ‖ u32 k ‖ s_k ‖ contributions in player_slot order)`. No one steers it: every server seed is fixed before any contribution, and a signature covers each contribution. A server can still abort a session whose first seeds it dislikes, which reputation and receipts answer.
- **Payloads.** A payload is a postcard list of commands, each its addressee, `0` for the players or `1` and a capability's index, and its bytes; the sim ignores a command it cannot decode. A command to the players is a select: `0` for units that take the place of those its player commanded, or `1` for more of them, then the units as a count and each stable id, in increasing id. A command to a capability holds its directive alone, which goes to the units its player commands at its place among the tick's commands.

### Server inputs

- **The log knows each player slot's controller.** It records each player slot's controller at every tick, from the player slot starts and the server inputs, with no package, and refuses a player packet for a player slot whose player is not the packet's. A server input is signed over `"campfire/server-input/v1" ‖ session id ‖ u64 tick ‖ u32 index ‖ input`.
- **A change is an `Admin` input of Rhai, planned.** `sim_eval` with a change logs its source as a server input, run in a role `admin` that holds the mode's calls and the admin verbs the registry lists, so a replay, the verifier and a report's replay run it again (02-engine-core.md, Everything outside is a recorded input); the session's terms say they admit admin changes, a ranked server refuses such terms, and a listing marks the session, as OpenRA's lobby option admits its dev orders and Factorio marks a save. Refused: typed admin orders, as OpenRA's 16, each a decoder beside a verb the script API holds; both, two ways to one change; writes into the world, which no log replays.

| Input | Fields | Effect |
| --- | --- | --- |
| `Bot` | `u32` player slot, payload bytes | The player slot's bot's commands, applied as a player's are |
| `Join` | `u32` player slot, delegation JSON | The player slot's controller becomes the delegation's player |
| `Renew` | `u32` player slot, delegation JSON | The player slot's player signs with a new session key from then on |
| `Leave` | `u32` player slot, reason (0 asked, 1 grace), what the player slot becomes (0 reserved, 1 a bot, 2 open) | The player slot's player leaves it |
| `Connected` | `u32` player slot | None; the log shows when a player's link came |
| `Disconnected` | `u32` player slot | None; the log shows when it went |

### Player inputs

- **Chain.** Each input links to the one before by `BLAKE3("campfire/input-hash/v1" ‖ previous hash ‖ u32 player_slot ‖ u64 seq ‖ u64 stamp tick ‖ payload)`, the first to the chain's root: the delegation's event id, or for a `Join` `BLAKE3("campfire/join-root/v1" ‖ delegation id ‖ u64 tick ‖ u32 index)`, so a player who joins again starts a chain none of their earlier packets extends. Neither link nor seq is sent: each side computes both, and the signature over the head shows the copies agree.
- **Signature.** The client signs the chain head once per packet, over `"campfire/input/v1" ‖ session id ‖ u32 player_slot ‖ u64 seq ‖ chain head hash`; the log keeps or refuses a packet whole.
- **Limits.** The terms fix the max payload length and the max inputs per tick; a client that keeps them is never refused, and a refusal ends the connection.
- **A session's terms hold every input.** A session's max payload length is at least `Release::LEAST_PAYLOAD_LEN`, 256 bytes, which holds the largest order to one unit, 51 bytes, and a bot script's mode input, which the load bounds to it; a server refuses to host, and a client to join, terms below it, so an input the engine makes too large to send is a logic error that crashes. Refused: a drop of each input past the terms while a match runs, which lost a player's order of 25 units under 64 bytes with a warning alone ([The size of an order to a group](../research/order-size.md)).
- **An input holds 1,024 bytes.** `InputRules::LAN` lets a payload hold 1,024 bytes, Zero Hour's retail bound on a message, which one of Campfire's 1,200-byte datagrams carries, as Zero Hour and Spring let one message fill about a packet ([The size of an order to a group](../research/order-size.md)); a player then holds at most 126,976 bytes of inputs on the server. Refused: 64 bytes, under which a move holds 24 units at most and fewer as ids rise.
- **Applied tick.** An input applies at `max(stamp tick, next tick)` or later, a player's inputs filling each tick up to the max in chain order; one the max input lead ahead is logged and not applied, so the chain stays whole.
- **A late input applies at the next tick.** An input that lands after its stamp applies at the next tick it can, in chain order, as an order is an event a player gave: the Source engine runs a player's late commands as they arrive, and a lockstep RTS waits for each; the max inputs per tick bound a burst. Its place, where the inputs before it leave room, lies within the max lead past the next tick: a packet with one past it is refused, as one sent ahead of time is, and a client holds its inputs while the server may hold the window's, so every logged input applies, as Factorio's server delays an input and never ignores it ([FFF #302](https://www.factorio.com/blog/post/fff-302)). Refused: a late input that never applies, which loses a player's earliest orders in a lag spike, as fits a shooter's input frame, a state the next frame gives again; the client's resend of one, which applies it after a later order the server applied. Inputs of one tick take effect in player slot order, then seq order, so the host cannot choose who acts first.
- **Acknowledgment.** For each packet the server sends the seq of its first input and each input's tick, as Gambetta's client-side prediction acknowledges each input.
- **Receipts.** About once a second the server signs `{session id, player_slot, delegation id, tick, seq, head}` over `"campfire/receipt/v1" ‖ …` for each player whose synced seq advanced, so a receipt proves those inputs survive any crash, though not a disk that corrupts the journal ([Sessions](08-sessions.md#receipts)).

## Verification

1. Check every delegation, chain link and signature, every bot and external input signature, and every applied tick against the delay rule.
2. Fetch the release the header names, checked against release events from at least k keys of the pinned set, or build it from its tag.
3. Fetch each package's index by fingerprint, or import each imported package from one's own copy, and check the fingerprints.
4. Check the seed reveal against the commitment, and each snapshot against its checkpoint: the first segment's, required, for a file that starts at a checkpoint.
5. Replay the inputs tick by tick from tick 0, or from the first segment's snapshot, each segment with its own seed.
6. Compare the state hash at each checkpoint and the final hash and outcome with the records.

- **A check proves** that the result and every checkpoint follow from the logged inputs on the named release and packages, that every player input was signed by its player, that no input is missing before a player's last logged chain head, and that every applied tick keeps the delay rule.
- **It does not prove** that the log holds every input after a player's last head, which receipts can disprove; that delays within the max came from the network; that bot and external inputs are honest; or that the host did not use hidden state.

## Nostr events

- **NIPs where a NIP says it.** Labels (NIP-32) for reputation, reports (NIP-56) for review requests, lists (NIP-51) for ban lists, classified listings (NIP-99) for the marketplace, Blossom (NIP-B7) for files; every other event is a campfire kind, proposed as a NIP once milestone 2 runs it, its fields fixed when milestone 2 builds it.
- **A license names the package id, not a fingerprint,** so it covers future versions of the same hero or skin.

| Event | Signed by | Content |
| --- | --- | --- |
| Server listing | Server key | Address, TLS key, tick rate, release tag, region, protocol version, modes and maps, import recipes, capabilities, rules summary, prices |
| Package announcement | Author key | Package id, version, fingerprint, license, download locations |
| Session log published | Server key | Session id, log and snapshot fingerprints, download locations, result |
| License | License key, with its delegation | Buyer main pubkey, package id, payment hash |
| Reputation statement | Any main or server key | Subject, session id, claim, optional log fingerprint and tick range |
| Review request | Any main or server key | Session id, reported pubkey, tick range, reason |
| Arbiter signature | Arbiter key | Session id, result, final state hash |
| Marketplace listing | Author key | Package id, price in sats, preview media, license terms |
| Engine release | Release keys, valid at the pinned threshold | Release tag, protocol version, per-platform hashes, download locations |
| Release revocation, key set | Release keys, at the threshold | Release tag and reason; new key set and threshold |
| Item export | Source server key | Export id, destination, item, owner, source session and tick, expiry |
| Ownership proof, item listing, sale receipt | Server or owner key | The item sale's records |
| Ban list | Server or group key | Banned pubkeys with reasons |
| Server group | Group key | Member servers, shared rules summary |

## Payment flows

- **The server drives its own wallet over NWC (NIP-47).** Hold invoices use the NWC hold invoice extension, and wager pools turn off when the wallet lacks it. The sim never waits on a payment: it sees only confirmed payment events, recorded as external inputs.
- **Wager pools lock stakes in hold invoices.** Each stake is held until the result, settled to the winners, or cancelled on an abort; above a stake the listing names, the result first needs the listed arbiters' signatures, each of whom replays the log.
- **Budgets for time-based charges.** A player's wallet grants an NWC connection with a budget the player can revoke; no specification defines budgets, so the client shows which wallets it has tested.

### Item sale

- **An atomic swap with no one holding the money.** The world's record of the transfer releases the hold invoice's preimage: the seller lists with a server-signed ownership proof, the server reserves the item and gives the seller a hash, the buyer pays a hold invoice of it, the server records the transfer and publishes a receipt with the preimage, and the seller settles with it. A host that publishes the preimage without the transfer, or the reverse, shows in the log and the receipts.

## Item export

- **Burn and attest,** as Circle's CCTP moves a token between chains: the source destroys the item by a recorded input and signs an export that names its one destination; the destination redeems it once, keeping the redeemed ids as state; an export not redeemed by its expiry returns to the source. It does not prove the source honest, so an item is worth the trust in the server that made it.
