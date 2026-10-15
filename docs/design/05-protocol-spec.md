# Campfire — Protocol Spec

## Scope and versioning

Everything a third party needs to read and verify session logs, and to build a storefront, a server browser or a wallet integration: keys and signatures; the session log; Nostr events for listings, packages, licenses, reputation and the marketplace; payment flows.

Not in the protocol: state replication between client and server (`net`), internal because both always run the same engine release. A client and a replay always come from the release the log names; the protocol lets anyone fetch that release and check the log with it. The protocol has its own version, which every log and event states.

## Encoding and cryptography

| Item | Rule |
| --- | --- |
| Binary encoding | `postcard` 1.x |
| Numbers in game data | 40.24 fixed point, as raw `i64` |
| Anything published as a file: package files and fingerprints, session logs, snapshots | SHA-256, 32 bytes, so [Blossom](https://github.com/hzrd149/blossom) servers host it as is |
| State hashes, and every other hash the protocol and the sim make for themselves | BLAKE3, 32 bytes, its input led by a domain tag `campfire/<name>/v<n>` |
| Signatures | Schnorr over secp256k1, as in Nostr |
| Nostr events | Standard Nostr JSON; payloads defined here |

**Hashes, ids and keys.** Every value of 32 bytes is one type, `Value32<K>`, of a marker `K`, an empty enum that implements `Meaning` and names the value, its alias carrying the docs, `pub type SessionId = Value32<SessionIdMeaning>`; a marker that also implements `ContentMeaning` gives its values `of(bytes)`, the SHA-256 of the bytes. One implementation gives every meaning the same calls, `new`, `as_bytes`, `get`, `of_hex`, `Display` and `FromStr` as 64 lowercase hex digits, the one spelling it reads back, serde as its bytes, order and `Hash`, and `Debug` as its name and hex; no value holds its bytes as a bare array. Two hashers make them: `ContentHash::of`, and its streaming `ContentHasher`, the SHA-256 of bytes, which a Blossom server names a blob by; and `DomainHasher::new(domain)`, BLAKE3 whose input starts with its domain tag, a `HashDomain` that each site declares as a constant, whose spelling the compiler checks, so no site forgets it or misspells it. A name says what a value is:

| Name | Is | Values |
| --- | --- | --- |
| `ContentHash` | the SHA-256 of a file's bytes, as a package's row lists it | each package file; an import's pinned archive |
| `…Fingerprint` | the `ContentHash` of an artifact published whole | `PackageFingerprint`, of a package's index; `SnapshotFingerprint`, of a snapshot |
| `…Hash` | a BLAKE3 digest of a domain | `StateHash`, `InputHash`, a journal frame's `FrameHash`; and `PartHash`, one state type's encoding, which the state hash folds after the type's name under its own domain, so a part needs none |
| `…Id` | an identity made by a hash | `SessionId`, of the terms; `DelegationId`, the Nostr event's id |
| `…Commitment` | a BLAKE3 digest that binds a secret before it shows | `SeedCommitment`, of the first server seed |
| a role's name | a key or a random value, no hash | `MainKey`, `TlsKey`, `SeedContribution`, `ConnectChallenge`, `ChannelBinding`; a seed in a `Secret` |

The primitives, `Bytes32`, `Value32`, `ContentHash`, `DomainHasher` and `Secret`, live in `common`; each marker lives in the crate that owns its concept. Every hash goes through the two hashers but the state hash's parts, which the sim's buffered `HashSink` makes, and the random stream's keyed BLAKE3, a key's function, not a domain's.

## Packages

- **The index.** A package is its index: one `(path, size, SHA-256)` row per file, sorted by path bytes, paths with `/` separators, in `package.index` at the package's root, which lists every other file and not itself. It is text, as Go's module hash (`dirhash`) and BagIt's manifests list a tree's files, so a reader and a diff show it: the line `campfire/package-index/v2`, then a line for each row, `<SHA-256> <size> <path>`, the SHA-256 in 64 lowercase hex digits, the size in decimal digits with no leading zero, the fields one space apart, each line ending in LF, the last too. A path is printable ASCII, a space among it but no tab or line end, so it takes the rest of its line and needs no escape. Its own code writes and reads it, no serializer's layout, so the bytes, and the fingerprint, change only with the format's version. Its fingerprint is the SHA-256 of the index's bytes, tag included. A path is names joined by `/`, none empty, `.` or `..`, none holding `\`; an index that lists another path, itself, two paths that differ only in case, a path out of order, or is not the one encoding of its rows does not read, so one package has one fingerprint. Packing refuses a link and a name no package path spells ([Package paths](03-game-scripting.md#game-package)). Whoever builds a package writes its index: the `import` app, a packing step, and for the workspace's test packages a test that fails when a checked-in index differs, which `CAMPFIRE_BLESS=1` writes again.
- **Reads on demand.** A load reads the index, then only the files the session needs: each package's manifest, `data/`, `scripts/` and `locale/`, and the session's map's directory; a client reads an asset when it draws it. The open lists each directory once and fails when a listed file is missing by its exact name, so a package is read whole or not at all. Each read checks the file against its row, no further than its size, failing with the path on a difference. A file the index does not list is no part of the package. So a package costs what its session reads, and the fingerprint still names every byte, as a git tree names its blobs.
- **No limits** of files or bytes: a client gives a session's files the memory they take, and trusts its servers to send sound packages.
- Each file is a separate blob, so an update downloads only changed files and packages share identical assets. On disk, a package's writer keeps one copy of identical files, as `nix-store --optimise` and pnpm's store do: a file whose SHA-256 an earlier file of the package has is a hard link to that file, or a copy where the file system makes no link. The index lists each path with its own row as before, so the fingerprint, every reader and the tree a person browses stay as they are; the index's hash is the link, and a package written by hand may keep its copies. A hard link is a file like any other to a walk, which refuses only a symbolic one. No tool may convert a package's line endings, so the repository keeps text LF on every OS.
- A client fetches a package's index by fingerprint, then the blobs it reads, from the Blossom servers in the package announcement. A game server also serves every package it pins.
- An **imported package** has no announcement and no blob: each machine makes it with the `import` app from its own copy, the same bytes on every OS for one install, game version and importer release ([Zero Hour](12-zero-hour.md#decisions), D4). A listing names it by fingerprint with its **import recipe**: the importer release, the game and its version.

## Keys

| Key | Owner | Signs |
| --- | --- | --- |
| Main key | Player (Nostr identity; ideally a remote signer, or a local file encrypted with [NIP-49](https://github.com/nostr-protocol/nips/blob/master/49.md)) | Session key delegations, licenses held, reputation statements |
| Session key | Player's client, one per session, short-lived, renewed by a new delegation | Chain heads of the player's inputs |
| Server key | Host (Nostr identity) | Listings, checkpoints, bot and external inputs, receipts, results |
| Author key | Content creator | Package announcements, license key delegations |
| License key | A storefront the author delegates to | Licenses |
| Group key | A group of hosts | Membership, shared ban lists |
| Release keys | The maintainers (or a fork's), offline; a launcher pins the set and its threshold k of n | Releases, revocations, key-set changes |

**Session id:** the hash of the session's terms, which the server fixes when it opens the session, before any player joins: `BLAKE3("campfire/session-id/v3" ‖ server key ‖ u32 tick rate ‖ u64 max input delay ‖ u64 max input lead ‖ u32 max payload length ‖ u32 max inputs per tick ‖ seed commitment ‖ u64 release tag length ‖ release tag ‖ mode fingerprint ‖ u64 map name length ‖ map name ‖ u64 dependency count ‖ dependency fingerprints ‖ u64 slot count ‖ u8 slot plan per slot ‖ u64 visibility length ‖ visibility)`, integers little-endian, a slot's plan 0 for a player, 1 for a bot, 2 for open. The release tag names the engine release; the fingerprints name the mode and its dependencies in load order; the map is one of the mode's. The visibility is the settings of [Replication audiences](14-replication-audiences.md) (D14, D15) as the mode and its host set them: a `u8` for each kind a mode may set, in the order `DataKind` declares them, its recipients beside the owner as bits (allies 1, enemies 2, spectators 4); the spectators' fog (0 `all`, 1 `teams`); what a player out of play sees (0 `team`, 1 `spectators`); and the most spectators the session admits as a `u32`. Every delegation and chain-head signature names the id, so the players sign the terms and a log cannot change them: a verifier computes the id from the header. The seed commitment is fresh for every session, so no two share an id.

**Session key delegation:** a Nostr event of kind 22710, signed by the main key, as remote signers (NIP-46) sign only events; the ephemeral range keeps a relay sent one by mistake from keeping it. It is never published: the log header holds it verbatim, so a verifier, and anyone who reads a published log, links every input to a player's identity. Each term is one tag of one value: `session_key`, `server_key`, `session_id` and `seed_contribution` in lowercase hex, and NIP-40's `expiration` in Unix seconds; other tags and the content are ignored. Its JSON holds at most 4,096 bytes. The contribution is the player's random share of every segment's seed, chosen after the server's seed commitment is fixed in the session id the delegation names, so no one can change it or choose it with the seed in view. Only the header delegation's contribution counts. The server checks the expiry against its clock at each join; a verifier, with no clock, does not.

**License key delegation:** the same shape, signed by the author key for a license key, with the package ids it may license and an expiry, so a storefront issues licenses while the author is offline.

## Connection

The connection is part of the protocol, as it binds the Nostr identities to the transport; the game traffic inside it (`net`) is not.

1. The server presents a raw Ed25519 key to TLS, with no certificate, derived from its own key ([QUIC transport](15-quic-transport.md), D6); the TLS key in its signed listing proves the client reached the server its key names. The listing names the session's tick rate, which the client's link ticks at from the start.
2. The client connects over QUIC and takes only the TLS key it pinned; a server answers a new address with a Retry first.
3. The server offers the session's terms, its grace period and restore window, and a random challenge, again until the client answers, for at most 10 s from the first offer, which goes out as the link starts: an honest client answers the first offer it receives, and the server closes a link still silent then with the close code `Unanswered`, so a connection that only keeps itself alive holds no link. A join the server refuses, for its delegation, its answer, a full lobby, no slot after the start, or a change of seat its log refuses, it logs with why, and tells the client by a word on the reliable channel, as it tells a client a newer login superseded ([Sessions](10-sessions.md#reconnect-and-late-join), Door): the client stops, as a join again would meet the same refusal, and ends the link with the close code `JoinRefused`; the server ends it itself 5 s after the word's first send, so a client that ignores the word holds its link no longer. A word and not only a close code, as an in-process link carries none. A spectator answers the offer as a player does, its join asking to watch: the server checks the answer as a player's, admits it while fewer spectators than the terms admit watch, else refuses it as a join, and tells it the Lightyear tick of the next sim tick; it holds no slot, and the log holds nothing of it ([Replication audiences](14-replication-audiences.md), D15). The client checks that the terms name the server key and tick rate the listing gave, its own release and mode packages, and a rate within the mode's range; then it delegates a fresh session key and replies with the delegation and a session-key signature over `"campfire/connect/v2" ‖ challenge ‖ channel binding`, the `tls-exporter` value of [RFC 9266](https://www.rfc-editor.org/rfc/rfc9266) that its TLS session gives both ends alike. The server checks the answer with its own end's binding and accepts only a delegation naming its own key, so a reply relayed from another TLS session is useless, even from a party that holds the server's TLS key, as its two sessions give two bindings ([QUIC transport](15-quic-transport.md), D11). Players take slots in the order their replies are accepted, those of one server frame by main key then session key, one slot for each main key; the newest login wins its slot ([Sessions](10-sessions.md#decisions), D10). The match starts when every slot is taken by a live connection; after it, the server keeps offering, and seats each reply in its slot or one the mode lets it take ([Reconnect and late join](10-sessions.md#reconnect-and-late-join)). Its answer names the slot, the Lightyear tick of the next sim tick and that tick, and the player's chain as the log holds it: its next seq and head, or none. A player who leaves says so; the server logs the leave and ends the connection.
4. A home server asks its router to forward its port, by PCP, NAT-PMP or UPnP-IGD, and its host forwards it by hand where no router answers ([QUIC transport](15-quic-transport.md), D12). LAN servers announce their signed listing over mDNS.
5. Against floods, cheap checks come first: QUIC address validation, limits per address from the session's seats ([QUIC transport](15-quic-transport.md), D13), and a packet's size before its signature.

## Session log

```
SessionLog
  header
    protocol version
    terms: server pubkey, tick rate, max input delay and max input lead (ticks),
           max payload length, max inputs per player per tick, seed commitment,
           engine release tag, mode package fingerprint, map name, dependency fingerprints,
           slot plan: player, bot or open per slot, visibility settings
           (the session id is their hash, and is not written)
    slots: each slot's start: a player's session key delegation (which names the main pubkey
           and carries the seed contribution), bot or open
  segments[]
    checkpoint (every segment but the first): segment, tick, state hash,
                snapshot fingerprint, log carry, server signature
    ticks[]: entries[] logged before the tick ran, each one of:
             player packet: slot, inputs[]: stamp tick, payload,
                            session-key signature over the chain head after the last input
             server input: the input, server signature at its tick and index
  entries[] logged after the last tick
  seed reveal: server seed of the last segment, which reveals every earlier one
               (added when the log is published)
  result (optional): tick, outcome, final state hash, server signature
```

**File.** A log file starts with the tag `campfire/session-log/v4`, which states its protocol version, then postcard: the header (the terms: server key, tick rate as a non-zero `u32`, max input delay and lead, max payload length, max inputs per tick, seed commitment, release tag as a string, mode fingerprint, map name as a string, dependency fingerprints and slot plan as lists, and the visibility as its bytes; then the slot starts as a `u32` count and, each, its plan's `u8` and for a player the delegation JSON); the `u32` count of segments, at least one, each its checkpoint record and 64-byte signature, but the first, which starts at tick 0 and has none, then the `u64` count of its ticks and the entries logged before each; the entries logged after the last tick; the seed reveal as an option; and the result with its 64-byte signature as an option. Entries go as a `u32` count, each as its kind's `u8`: a player packet, 0, as `u32 slot`, its inputs as a `u32` count of `u64 stamp tick, payload bytes`, and the 64-byte signature; a server input, 1, as the input and its 64-byte signature. A reader checks every delegation, records every entry again, so it checks each chain link and signature, and accepts only the canonical encoding, encoding what it decoded and comparing the bytes, so one log has one file and one fingerprint. A test pins, for each tag, a small log of every kind of entry in `campfire-protocol`'s `tests/golden/session-log/`: the current tag's file must be what the encoder writes, `CAMPFIRE_BLESS=1` writes only a missing one, and each older tag's file is refused, so the format changes only with its tag. A verifier replays only a log of its own release, with the packages the terms name, at a rate within the mode's range, each other refused by its own error, the release checked before any package is read.

**Checkpoints and results.** A checkpoint at the boundary before tick `t` ends the last segment, of at least one tick, and starts the next, whose seed the sim draws from `t` on. Its record is `{u32 segment, u64 tick, state hash, snapshot fingerprint, log carry}`, signed by the server key over `"campfire/checkpoint/v1" ‖ session id ‖ postcard of the record`; the snapshot fingerprint is the SHA-256 of its bytes. The log carry is the log's own state at the boundary, so a segment verifies from its checkpoint alone: for each slot its controller (a player, as their current delegation JSON, slot, chain head and next seq; a bot; open; or reserved), the main key of its last leaver as an option, the player's stamp counts (the last stamp as an option, that stamp's inputs, all inputs) and spill (the last tick their inputs fill, and how many fill it); then the inputs logged before the boundary and due after, each as its tick, `u32 slot`, stamp and payload, in apply order. A log refuses a record of another segment or tick than the next, after a segment of no tick, or whose carry is not its own state. The result is `{u64 tick, outcome, final state hash}`, the outcome `won` with a team's index, `draw` or `aborted`, signed over `"campfire/result/v1" ‖ session id ‖ postcard of the result`, its tick the one after the last that ran: `won` or `draw` as the mode ended the match, `aborted` when it did not.

### Input sources

| Source | Signed by | Payload |
| --- | --- | --- |
| Player | Session key | Commands, each in its capability's format |
| Bot | Server key | A player input's format |
| External | Server key | Connect or disconnect, carry load, admin command, payment event, calendar time |

- A match starts one segment at tick 0. A world adds a checkpoint every few minutes; a save adds one, and a load starts a new segment from a save's checkpoint, which may be earlier than the last tick logged: the log goes back to the save, whose segment keeps its number and seed, so the segments stay consecutive ([Saves and loads](10-sessions.md#saves-and-loads)). A converted segment names both releases ([Saves](02-engine-core.md#saves)).
- **Seed chain.** At the session's opening the server fixes every segment's server seed as a one-way hash chain, the scheme of Lamport's one-time passwords: a random root `r` and a length `N`, the most segments, and `s_k = C^(N−1−k)(r)` with `C(x) = BLAKE3("campfire/seed-chain/v1" ‖ x)`, so each seed is the hash of the next. The terms commit to `C(s_0)`. Revealing `s_k` reveals every earlier seed and no later one, checked by `k + 1` hashes.
- **Seed.** Segment `k`'s seed is `BLAKE3("campfire/segment-seed/v1" ‖ u32 k ‖ s_k ‖ contributions in slot order)`. No one steers it: every server seed is fixed before any contribution and tick, no player knows an unrevealed one, and a signature covers each contribution. A server can still abort a session whose first seeds it dislikes, which reputation and receipts answer. A verifier takes a seed only from the reveal, checked against the commitment; a seed is revealed only with its segment, as it predicts its hidden outcomes. A session that runs out of chain ends.
- Players who join later, and renewals, come by signed server inputs carrying the delegation; a renewal keeps the player's chain.
- A payload is a postcard list of commands, each its capability's index and its bytes ([Commands](04-capabilities/00-overview.md#commands)). The log keeps any payload within the max length; the sim ignores an undeclared capability's command and one that does not decode.

### Server inputs

The log records each slot's controller at every tick, a player, a bot, open, or reserved for a leaver, from the slot starts and the server inputs, with no package. It refuses a player packet for a slot whose player is not the packet's, and a bot input for a slot no bot plays. A server input is logged before the tick it applies in, with one server signature over `"campfire/server-input/v1" ‖ session id ‖ u64 tick ‖ u32 index ‖ input`, little-endian, `tick` the next tick when logged, `index` the count of server inputs logged before that tick, `input` its postcard: its kind's index in the table, then its fields.

| Input | Fields | Effect |
| --- | --- | --- |
| `Bot` | `u32` slot, payload bytes | The slot's bot's commands, applied as a player's are, in slot order, spilling past the max inputs per tick within the max input lead |
| `Join` | `u32` slot, delegation JSON | The slot's controller becomes the delegation's player, whose chain starts from its id |
| `Renew` | `u32` slot, delegation JSON | The slot's player, the same main key, signs with the new session key from then on |
| `Leave` | `u32` slot, reason (0 asked, 1 grace), what the slot becomes (0 reserved, 1 a bot, 2 open) | The slot's player leaves it |
| `Connected` | `u32` slot | None; the log shows when a player's link came |
| `Disconnected` | `u32` slot | None; the log shows when it went |

A `Join`'s or `Renew`'s delegation names the session and the server; its contribution is ignored. A `Join` takes an open slot, a bot's, or one whose leaver it brings back; a `Renew` keeps the main key; `Connected`, `Disconnected`, `Renew` and `Leave` need a player in the slot. A player's inputs logged before a `Join` or `Leave` of their slot and due later never apply; a `Renew` keeps them. What a `Join` may take and a `Leave` leave, the mode's `[players]` says, which the runner checks as it records, so a verifier checks it too ([Slots and controllers](10-sessions.md#slots-and-controllers)).

### Player inputs

- **Chain.** Each input links to the one before by a hash, the first to the delegation's event id. Neither link nor seq is sent or logged: each side computes both, and the signature over the head shows the copies agree, so a later signature covers every earlier input. The hash is BLAKE3 of `"campfire/input-hash/v1" ‖ previous hash ‖ u32 slot ‖ u64 seq ‖ u64 stamp tick ‖ payload`, little-endian, seq counting a player's inputs from 0.
- **Signature.** The client signs the chain head once per packet, over `"campfire/input/v1" ‖ session id ‖ u32 player slot ‖ u64 seq ‖ chain head hash`, little-endian, seq the packet's last input's. The log keeps or refuses a packet whole.
- **Limits.** The header fixes the max payload length and max inputs per tick. A player's stamps never go back; a stamp and a packet carry at most the max inputs per tick; the log refuses a packet that breaks one of these, a payload over the max, inputs past the max for each tick up to the max input lead past the next, or a packet that would take the log past 4 GiB, each checked before any input is hashed. Each is the client's own doing, so a client that keeps them is never refused; a refusal ends the connection. The client stamps an input with the tick it predicts it in, or its last stamp when later, and holds an order while its stamp would pass the max input lead beyond the last tick the server sent.
- **Applied tick.** The server applies an input at `max(stamp tick, next tick)` or later: a player's inputs fill each tick up to the max inputs per tick, in chain order, and the rest wait, so a burst applies over the ticks that follow. An input more than the max input delay after its stamp is logged late and not applied; one more than the max input lead ahead is logged early and not applied, so the chain stays whole. The log groups inputs by the tick that was next on arrival, so the applied tick, lateness and earliness follow from it and are not stored. Inputs of one tick take effect in slot order, then seq order, so the host cannot choose who acts first.
- **Acknowledgment.** For each packet it logs, the server sends the player the seq of its first input and each input's tick, or none for one late or early, as Gambetta's client-side prediction acknowledges each input; a rollback replays each input in that tick, or never.
- **Receipts.** About once a second, for each player whose durable seq advanced, the server signs `{session id, slot, delegation id, tick, seq, head}` over `"campfire/receipt/v1" ‖ session id ‖ u32 slot ‖ delegation id ‖ u64 tick ‖ u64 seq ‖ head` and sends it; seq is the player's last input whose journal record is synced, so a receipt proves those inputs survive any crash ([Receipts](10-sessions.md#receipts)).

## Verification

1. Check every delegation, chain link and chain-head signature, every bot and external input signature, and every applied tick against the delay rule.
2. Fetch the release the header names, checked against release events from at least k keys of the pinned set, or build it from its tag.
3. Fetch each package's index by fingerprint, or import each imported package from one's own copy of its game, and check the fingerprints; each file is checked against its index as it is read.
4. Check the last segment's seed reveal against the commitment, which checks every earlier seed. With the snapshots at hand, check each fingerprint, and that each restores to its checkpoint's state hash.
5. Replay the inputs tick by tick from tick 0, each segment with its own seed.
6. Compare the state hash at each checkpoint with its record's, and the final hash and outcome with the result's.

**A check proves:** the result and every checkpoint follow from the logged inputs, on the named release and packages; every player input was signed by its player; no input is missing before a player's last logged chain head; every applied tick keeps the delay rule.

**It does not prove:**

- that the log holds every input a player sent after their last logged head: their receipts can prove it does not;
- that delays within the max input delay came from the network and not the host;
- that bot and external inputs are honest: the server signs them;
- that the host did not use hidden state: the server sees everything.

## Nostr events

Where a NIP says what an event says, campfire uses it: labels ([NIP-32](https://github.com/nostr-protocol/nips/blob/master/32.md), kind 1985) for reputation, reports ([NIP-56](https://github.com/nostr-protocol/nips/blob/master/56.md), kind 1984) for review requests, lists ([NIP-51](https://github.com/nostr-protocol/nips/blob/master/51.md)) for ban lists, classified listings ([NIP-99](https://github.com/nostr-protocol/nips/blob/master/99.md), kind 30402) for the marketplace, and Blossom ([NIP-B7](https://github.com/nostr-protocol/nips/blob/master/B7.md)) for files. Every other event is a campfire kind in the range NIP-01 gives its behavior; campfire proposes its kinds as a NIP once milestone 2 runs them, and fixes each event's fields when milestone 2 builds it.

| Event | Signed by | Content | Kind |
| --- | --- | --- | --- |
| Server listing | Server key | Address, TLS key, tick rate, release tag, region, protocol version, modes (fingerprints) and their maps, each imported package's import recipe, capabilities, rules summary (the visibility settings among them), prices | Campfire, addressable: one per server |
| Package announcement | Author key | Package id (author + name), version, fingerprint, license, download locations | Campfire, regular |
| Session log published | Server key | Session id, log and snapshot fingerprints, download locations, result | Campfire, regular |
| License | License key, with its delegation | Buyer main pubkey, package id, payment hash | Campfire, regular |
| Reputation statement | Any main or server key | Subject pubkey, session id, claim (paid out, log published, cheated), optional log fingerprint and tick range | NIP-32 label, campfire's namespace |
| Review request | Any main or server key | Session id, reported pubkey, tick range, reason | NIP-56 report, with the session tags |
| Arbiter signature | Arbiter key | Session id, result, final state hash | Campfire, regular |
| Marketplace listing | Author key | Package id, price in sats, preview media, license terms | NIP-99: one per package |
| Engine release | One release key; valid once k keys of the pinned set sign the same content | Release tag, protocol version, per-platform hashes of the unsigned build outputs, download locations | Campfire, regular |
| Release revocation | One release key; valid at the same threshold | Release tag, reason | Campfire, regular |
| Release key set | One key of the current set; valid at its threshold | New key set and threshold | Campfire, regular |
| Item export | Source server key | Export id, destination server key, item, owner's main pubkey, source session and tick, expiry | Campfire, regular ([Item export](#item-export)) |
| Ownership proof | Server key | Item id, owner's main pubkey, session and tick, expiry | Campfire, regular ([Item sale](#item-sale)) |
| Item listing | Owner's main key | Item id, its server key, price in sats, the ownership proof | NIP-99: one per item |
| Sale receipt | Server key | Item id, seller and buyer main pubkeys, price, payment hash, preimage, session and tick | Campfire, regular ([Item sale](#item-sale)) |
| Ban list | Server or group key | Banned pubkeys, each with a reason and optional session id | NIP-51 set: one per signer |
| Server group | Group key | Member server pubkeys, shared rules summary | Campfire, addressable: one per group |

A license names the package id, not a fingerprint, so it covers future versions of the same hero or skin.

## Payment flows

The server talks to its own wallet over NWC ([NIP-47](https://github.com/nostr-protocol/nips/blob/master/47.md)). Hold invoices are in the [NWC hold invoice extension](https://github.com/nostr-wallet-connect/nwc/blob/main/03.md), so the server checks `get_info` and turns wager pools off when the wallet lacks it. The sim never waits on a payment: it sees only confirmed payment events, recorded as external inputs.

**Wager pool (and spectator pool)**

1. Waiting: each player gives a payout Lightning address; the server creates a hold invoice for their stake (`make_hold_invoice`).
2. The player pays; on `hold_invoice_accepted` the server records a stake-locked event.
3. Result: the server settles all stakes (`settle_hold_invoice`) and pays winners, less the host fee; between settle and payout the host holds the pool. Above a stake the listing names, the result first needs the signatures of the listing's arbiters, each of whom replays the log.
4. Aborted: the server cancels every hold invoice (`cancel_hold_invoice`), and the funds return.

**Entry fee:** a normal invoice paid during waiting, recorded as an external input.

**Time-based and per-event charges:** the player's wallet grants an NWC connection with a spending budget, which the player can revoke; the server charges as prices come due, each charge recorded. No NWC specification defines budgets, so the client shows which wallets it has tested.

**Rewards:** the server pays the player's Lightning address and records it.

### Item sale

A player sells a world's tradable item to another for sats, with no one holding the money between: only the world's record of the transfer releases the hold invoice's preimage, which the NWC extension lets a wallet take from the one who chose it.

1. **List.** The seller publishes an item listing with the price and an expiring ownership proof the world's server signed.
2. **Reserve.** The buyer asks the server to buy. The server checks that the type is tradable, the seller owns it and no other sale holds it, records a reservation, so the item can be neither used nor moved, picks a random preimage and gives its hash to the seller. A host fee is a separate invoice the buyer pays first.
3. **Invoice.** The seller's wallet makes a hold invoice for the price with that hash; the buyer pays it, held, and the seller tells the server.
4. **Transfer.** The server records the transfer to the buyer's main key and publishes a sale receipt carrying the preimage.
5. **Settle.** The seller's wallet settles with the preimage; the buyer's proof of payment is the same preimage.

- **Timeouts.** A reservation the seller does not confirm within the host's time ends by a recorded input, and the seller's wallet cancels its invoice. The invoice's expiry exceeds the reservation by a margin the host sets, so a seller offline at step 5 can still settle.
- **Who can cheat whom.** A seller who lies that a payment is held gives the item away. A buyer cannot take the item without paying first. A host that publishes the preimage without recording the transfer, or records it and withholds the preimage, shows in the published log and the receipts, and is answered by reputation.
- **Across worlds.** An item for sale in another world moves there first, by item export.

**Failed payouts:** the server retries for a period the host sets, then records the payout as unpaid in the result; the player can publish a "not paid" statement.

## Item export

An item leaves one world for another by burn and attest, as Circle's [CCTP](https://chain.link/article/burn-and-mint-transfer) moves a token between chains: the transfer names its one destination, so no shared ledger is needed.

1. **Burn.** The player asks to export an item to a destination server key. The source destroys it by a recorded input and signs an **item export** event: a unique export id, the destination key, the item (package id, type, state), the owner's main pubkey, the source session and tick, and an expiry.
2. **Redeem.** The destination accepts the event only when it names that server, comes from a server the host trusts, and has not expired by the calendar time it records. The redemption is a recorded input: the item appears, and its export id joins the world's redeemed ids, which are state, so a second redemption is refused.
3. **Return.** An export not redeemed by its expiry returns to the source, redeemed there after a margin the host sets for clock skew.

It proves that no honest server redeems one export twice, and that no two honest servers both hold the item. It does not prove the source honest: a source can mint items at will, so an item is worth the trust in the server that made it.
