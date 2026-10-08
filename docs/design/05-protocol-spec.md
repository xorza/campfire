# Campfire — Protocol Spec

## Scope and versioning

Everything a third party needs to read and verify session logs, and to build a storefront, a server browser or a wallet integration:

- keys and signatures;
- the session log (what a replay needs);
- Nostr events for listings, packages, licenses, reputation and the marketplace;
- payment flows.

Not in the protocol: state replication between client and server (`net`), which is internal because both always run the same engine release. So a game client, and the replay itself, always come from the engine release the log names; the protocol lets anyone fetch that release and check the log with it.

The protocol has its own version, separate from engine releases. Every session log and event states the protocol version it uses.

## Encoding and cryptography

| Item | Rule |
| --- | --- |
| Binary encoding | `postcard` 1.x (stable wire format) |
| Numbers in game data | 40.24 fixed-point, stored as raw `i64` |
| Anything published as a file: package files, package fingerprints, session logs, snapshots | SHA-256, 32 bytes, so [Blossom](https://github.com/hzrd149/blossom) servers host it as is |
| State hashes | BLAKE3, 32 bytes |
| Signatures | Schnorr over secp256k1, as in Nostr |
| Nostr events | Standard Nostr JSON; payloads inside are defined here |

## Packages

- A package is a file list: one `(path, size, SHA-256)` row per file, sorted by path bytes, paths in UTF-8 with `/` separators. Its fingerprint is the SHA-256 of the postcard-encoded list. A path is names joined by `/`, each one not empty, not `.` or `..`, and holding no `\`, so a path has one spelling and stays in its package; a package with a file no such path names, or with a link, does not read. A load reads a package's files once into memory, fingerprints those bytes and parses the same bytes, so what it parses is what the fingerprint names.
- Each file is a separate blob, so an update downloads only changed files and packages share identical assets.
- A package is its bytes: no tool may convert its line endings, so the repository keeps text LF on every OS.
- A client fetches blobs from the Blossom servers in the package announcement. A game server also serves every package it pins, so a session never depends on a mirror.

## Keys

| Key | Owner | Signs |
| --- | --- | --- |
| Main key | Player (Nostr identity; ideally held by a remote signer, or in a local file encrypted with [NIP-49](https://github.com/nostr-protocol/nips/blob/master/49.md)) | Session key delegations, licenses held, reputation statements |
| Session key | Player's client, one per session, short-lived; a long session renews it with a new delegation before it expires | Chain heads of the player's inputs |
| Server key | Host (Nostr identity) | Listings, checkpoints, bot and external inputs, input receipts, results |
| Author key | Content creator | Package announcements, license key delegations |
| License key | A storefront or service the author delegates to | Licenses issued |
| Group key | A group of hosts | Group membership, shared ban lists |
| Release keys | The project's maintainers (or any fork's), kept offline; a launcher pins the set and its threshold k of n | Engine release, release revocation and key-set change events |

**Session id:** the hash of the session's terms, which the server fixes when it opens the session, before any player joins: `BLAKE3("campfire/session-id/v1" ‖ server key ‖ u32 tick rate ‖ u64 max input delay ‖ u64 max input lead ‖ u32 max payload length ‖ u32 max inputs per tick ‖ seed commitment ‖ u64 release tag length ‖ release tag ‖ mode fingerprint ‖ u64 dependency count ‖ dependency fingerprints ‖ u64 slot count ‖ u8 slot plan per slot)`, integers little-endian, a slot's plan 0 for a player, 1 for a bot and 2 for open. The release tag names the engine release the session runs on; the fingerprints name the mode package and its dependencies, in the mode's load order, which decides which override wins. Every delegation and every chain-head signature names the id, so the players sign the terms, and a log cannot change them: a verifier computes the id from the header and refuses a delegation that names another. The seed commitment is fresh for every session, so no two sessions share an id.

**Session key delegation:** a Nostr event of a Campfire kind, signed by the main key, with tags for session pubkey, server pubkey, session id and expiry. It is a Nostr event because remote signers (NIP-46) sign only events. It is never published to relays; the session log header holds it verbatim, so a verifier can link every input to a player's identity, and so can anyone who reads a published log: a player who wants play kept apart plays under another identity ([Identity](01-campfire-design.md#decentralized-server-network)). The main key never enters the game. The kind is 22710 for now, in the ephemeral range so a relay sent one by mistake does not keep it. Each term is one tag of one value: `session_key`, `server_key`, `session_id` and `seed_contribution` in lowercase hex, and NIP-40's `expiration` in Unix seconds; other tags and the content are ignored. Its JSON holds at most 4,096 bytes, several times what one with no content and no other tag takes, as the log keeps it whole. The contribution is the player's random share of every segment's seed: the player chooses it when it delegates, after the server's seed commitment is fixed in the session id the delegation names, so the main key signs the contribution, and no one can change it or choose it with the seed in view. Only the contribution of the delegation in the header counts; a renewal's is ignored. The server checks the expiry against its clock when the player connects; a log has no clock, so a verifier does not.

**License key delegation:** the same shape, signed by the author key for a license key, with tags for the package ids it may license and an expiry. It lets a storefront issue licenses while the author is offline.

## Connection

The connection is part of the protocol, because it binds the Nostr identities to the transport; the game traffic inside it (`net`) is not.

1. The server uses a self-signed TLS certificate; the hash in its signed listing proves the client reached the server its key names. The listing also names the session's tick rate: the client's link ticks at it from the start, before the terms come, as Lightyear reads its tick length once.
2. The client connects over QUIC (WebTransport) and checks the certificate hash.
3. The server offers the session's terms, its grace period and restore window, and a random challenge, and sends the same offer again until the client answers. The client checks that the terms name the server key and the tick rate the listing gave, its own release and mode packages, and a rate within the mode's range; then it delegates a fresh session key in that session and replies with its delegation and a session-key signature over `"campfire/connect/v1" ‖ challenge ‖ certificate hash it verified`. The server accepts only its own certificate hash and a delegation naming its own key, so a reply relayed from a connection to another server is useless there. This binds to the server certificate, like the `tls-server-end-point` channel binding of [RFC 5929](https://www.rfc-editor.org/rfc/rfc5929); a TLS exporter would be stronger, but Lightyear's WebTransport layer does not expose one. Players take slots in the order their replies are accepted, one slot for each main key: a reply whose main key holds a slot takes that slot over, and the server tells the older connection that a newer login took it, then ends it, as the newest login wins. A player whose connection ends before the match starts frees their slot for the next reply, and the match starts when every slot is taken by a live connection. After the start the server keeps offering, and seats each reply as [Sessions](10-sessions.md#reconnect-and-late-join) says: in the slot its main key holds or left, else in an open or a bot's slot the mode lets it take. Its answer names the slot, the Lightyear tick of the next sim tick and that tick, and the player's chain as the log holds it: its next seq and head, or none for a chain that starts from the delegation's id. A player who leaves says so, and the server logs the leave at once and ends the connection.
4. Home hosts forward a port or use UPnP. LAN servers also announce their signed listing over mDNS.
5. Against floods, cheap checks come first: QUIC address validation, limits on connections and packets per address, and a packet's size limits before its signature.

## Session log

```
SessionLog
  header
    protocol version
    terms: server pubkey, tick rate, max input delay and max input lead (ticks),
           max payload length, max inputs per player per tick, seed commitment,
           engine release tag, mode package fingerprint + dependency fingerprints,
           slot plan: player, bot or open per slot
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

**File.** A session log file starts with the tag `campfire/session-log/v2`, which states its protocol version, and continues in postcard: the header (the terms: server key, tick rate as a non-zero `u32`, max input delay and lead, max payload length, max inputs per tick, seed commitment, release tag as a string, mode fingerprint, the dependency fingerprints as a list, and the slot plan as a list; then each slot's start as a `u32` count and, each, its plan's `u8` and, for a player, the delegation JSON, which carries the seed contribution); the `u32` count of segments, at least one, each as its checkpoint record and the record's 64-byte signature, but the first, which starts at tick 0 and has none, then the `u64` count of its ticks and the entries logged before each; the entries logged after the last tick; the seed reveal as an option; and the result, with its 64-byte signature, as an option. Entries go as a `u32` count, then each as its kind's `u8`. A player packet, 0, goes as `u32 slot`, its inputs as a `u32` count and `u64 stamp tick, payload bytes` each, and the 64-byte signature; a server input, 1, as the input and its 64-byte signature. A reader checks every delegation and records every entry again, so it checks each chain link and signature, and accepts only the canonical encoding: postcard itself accepts an overlong varint, so a reader encodes what it decoded and compares the bytes. One log therefore has one file and one fingerprint. A verifier then replays the decoded log's ticks from what it holds, with no check done twice. It replays only a log of its own engine release, with the mode and dependencies it holds by the fingerprints the terms name, at a tick rate within the mode's range; any other log it refuses, each with its own error, and it checks the release before it reads any package, as another release's packages need not read in its own. A package under the verifier's store that does not read is reported and costs no other package.

**Checkpoints and results.** A checkpoint at the boundary before tick `t` ends the last segment, which holds at least one tick, and starts the next: from `t` on, the sim draws from that segment's seed. Its record is `{u32 segment, u64 tick, state hash, snapshot fingerprint, log carry}`, the server key's Schnorr signature over `"campfire/checkpoint/v1" ‖ session id ‖ postcard of the record`, where the snapshot fingerprint is the SHA-256 of the snapshot's bytes. The log carry is the log's own state at the boundary, so a segment verifies from its checkpoint alone: for each slot, its controller (a player, as the delegation JSON of their current key and their chain's slot, head and next seq; a bot; open; or reserved), the main key of the player who left it last as an option, the player's stamp counts (the last stamp as an option, the inputs of that stamp, the inputs in all) and their spill (the last tick their inputs fill, and how many fill it); then the inputs logged before the boundary and due from it on, each as the tick it applies in, `u32 slot`, stamp and payload, in the order they apply. A log refuses a record of another segment than the next, at another tick than the next, after a segment of no tick, or whose carry is not its own state there. The result is `{u64 tick, outcome, final state hash}`, the outcome `won` with the index of a team, `draw` or `aborted`, signed over `"campfire/result/v1" ‖ session id ‖ postcard of the result`; its tick is the tick after the last that ran. The server signs it when the session ends: `won` or `draw` as the mode ended the match, `aborted` when it did not.

**Input sources**

| Source | Signed by | Payload |
| --- | --- | --- |
| Player | Session key | Commands, each in its capability's format (order, input frame, mode input) |
| Bot | Server key | Same format as a player input |
| External | Server key | Player connect or disconnect, carry load, admin command, payment event, calendar time |

- A match starts one segment at tick 0 from the initial state. A persistent world adds a checkpoint every few minutes; a save adds one, and a load starts a new segment from a save's checkpoint, which may be earlier than the last tick logged: the log goes back to the save, whose segment keeps its number and seed, so the segments stay consecutive ([Saves and loads](10-sessions.md#saves-and-loads)). A segment converted to a newer release names both releases ([Saves](02-engine-core.md#saves)).
- **Seed chain.** When it opens the session, the server fixes the server seed of every segment at once, as a one-way hash chain, the scheme of Lamport's one-time passwords and of provably fair games: it picks a random root `r` and a length `N`, the most segments the session may have, and segment `k`'s server seed is `s_k = C^(N−1−k)(r)`, where `C(x) = BLAKE3("campfire/seed-chain/v1" ‖ x)`. So `s_(N−1) = r`, and each seed is the hash of the next. The terms commit to `C(s_0)`. Revealing `s_k` reveals every earlier seed and no later one, and a verifier checks it alone: `k + 1` hashes lead it to the commitment.
- **Seed.** Each player's delegation carries a random seed contribution, which the player signs. Segment `k`'s seed is `BLAKE3("campfire/segment-seed/v1" ‖ u32 k ‖ s_k ‖ contributions in slot order)`. No one can steer any segment's seed: every server seed is fixed before any contribution and before any tick, a player knows no unrevealed server seed, and a signature covers every contribution. A server can still abort a session whose first seeds it dislikes, once it holds every delegation; an aborted session is what reputation and receipts answer. A verifier takes a seed only from the log: from the reveal, checked against the commitment. A seed is revealed only when its segment is published, because it predicts every hidden random outcome of the segment; the chain keeps the next segments' secret. A session that runs out of chain ends.
- Players who join later, and session key renewals, are added through a signed server input carrying the delegation ([Server inputs](#server-inputs)). A renewal does not restart the player's input chain; the new key signs the chain heads from then on.
- Bot and external inputs are signed by the server key, one signature per input.
- A payload is a postcard list of commands, each the owning capability's index in the engine's fixed list and the command's bytes in that capability's format. The log keeps any payload within the max payload length; the sim ignores a command of a capability the mode did not declare, and one that does not decode.

**Server inputs**

The log records which controller each slot has at every tick, a player, a bot, open, or reserved for a player who left, from the header's slot starts and the server inputs that change them, with no package. It refuses a player packet for a slot whose player is not the packet's, and a bot's input for a slot no bot plays. A server input is logged before the tick it applies in, with one Schnorr signature by the server key over `"campfire/server-input/v1" ‖ session id ‖ u64 tick ‖ u32 index ‖ input`, little-endian, where `tick` is the next tick when it was logged, `index` counts the server inputs logged before that tick from 0, and `input` is its postcard encoding: its kind's index in the table, then its fields in order.

| Input | Fields | Effect |
| --- | --- | --- |
| `Bot` | `u32` slot, payload bytes | Commands of the slot's bot, applied in that tick as a player's are, in slot order; a payload within the max length, at most the max inputs per tick in a tick |
| `Join` | `u32` slot, delegation JSON | The slot's controller becomes the delegation's player, whose chain starts from its id |
| `Renew` | `u32` slot, delegation JSON | The slot's player, the same main key, signs with the new session key from then on |
| `Leave` | `u32` slot, reason (0 asked, 1 grace), what the slot becomes (0 reserved, 1 a bot, 2 open) | The slot's player leaves it |
| `Connected` | `u32` slot | None; the log shows when a player's link came |
| `Disconnected` | `u32` slot | None; the log shows when a player's link went |

A delegation in a `Join` or a `Renew` must name the session and the server, as the header's do; its seed contribution is ignored. A `Join` takes an open slot, a bot's, or a slot whose leaver it brings back; a `Renew` keeps the slot's main key; `Connected`, `Disconnected`, `Renew` and `Leave` need a player in the slot. A player's inputs logged before a `Join` or a `Leave` of their slot and due later never apply; a `Renew` keeps them. What a mode lets a `Join` take and a `Leave` leave, its `[players]` says, which the runner checks as it records, so a verifier checks it too ([Sessions](10-sessions.md#slots-and-controllers)).

**Player inputs**

- **Chain.** Each input of a player is linked to the one before it by a hash; the first links to the delegation's event id. Neither the link nor the seq is sent or logged: the client and the server each compute both from their own copy of the chain, and the signature over the head shows that the copies agree. A later signature therefore covers every earlier input, and a missing, reordered or altered input makes it fail. The hash is BLAKE3 of `"campfire/input-hash/v1" ‖ previous hash ‖ u32 slot ‖ u64 seq ‖ u64 stamp tick ‖ payload`, little-endian; seq counts a player's inputs from 0.
- **Signature.** The client signs the chain head once per packet, over `"campfire/input/v1" ‖ session id ‖ u32 player slot ‖ u64 seq ‖ chain head hash`, little-endian, where seq is the packet's last input's. The log keeps a packet whole or refuses it whole, with its signature, so every logged input is signed.
- **Limits.** The header fixes the max payload length and the max inputs per tick. A player's stamps never go back, a stamp carries at most the max inputs per tick, and so does a packet; the log refuses a packet that breaks one of these, a payload over the max length, inputs in all past the max inputs per tick for each tick up to the max input lead past the next, and a packet that would take the log past the 4 GiB its positions reach, as it refuses a broken link or signature. Each is checked before any input is hashed. Every one of these is the client's own doing, whatever the network does to its packets, so a client that keeps the limits is never refused: it stamps at most the max inputs per tick in one tick, and holds the orders past them for its next. A refusal ends the connection: the client's chain then differs from the log's. Together they bound how fast a player can grow the log.
- **Applied tick.** The server applies an input at `max(stamp tick, next tick)`, or later: a player's inputs fill each tick up to the max inputs per tick, in chain order, and the rest wait for the next tick, so packets that a network held and delivers together apply over the ticks that follow, none refused. An input that arrives more than the max input delay after its stamp is logged as late and not applied, so the chain stays whole. An input stamped more than the max input lead ahead of the next tick is logged as early and not applied, the same way: a predicting client stamps a few ticks ahead, and the server holds each input until its tick, so the lead bounds what a client can make it hold. The log groups inputs by the tick that was next when each arrived, so the applied tick, the wait, lateness and earliness follow from the log and are not stored: a verifier recomputes them and cannot be given wrong ones. The inputs applied in one tick take effect in slot order, then seq order, whatever order they arrived in, so the host cannot choose who acts first in a tick.
- **Receipts.** About once a second, the server signs `{session id, player slot, delegation id, tick, seq, chain head hash}` and sends it to the client, which keeps it. The seq is the player's last input whose journal record the server synced. A receipt proves the server logged every input up to that seq durably, so no crash loses them ([Receipts](10-sessions.md#receipts)).

## Verification

1. Check every delegation, every chain link and chain-head signature, every bot and external input signature, and every applied tick against the delay rule.
2. Fetch the engine release named in the header, checked against engine release events from at least k keys of the pinned set, or build it from its tag.
3. Fetch packages by fingerprint and check the fingerprints.
4. Check the seed reveal of the last segment against the commitment, which checks every earlier segment's seed. With the snapshots at hand, check each one's fingerprint, and that it restores to its checkpoint's state hash.
5. Replay the inputs tick by tick from tick 0, each segment with its own seed.
6. Compare the state hash at each checkpoint with its record's, and the final hash and the outcome with the result's: a `won` or a `draw` must be how the mode ended the match.

**What a successful check proves:** the result and every checkpoint follow from the logged inputs, on the named engine release and packages; every player input was signed by that player; no input is missing before a player's last logged chain head; and every applied tick keeps the delay rule.

**What it does not prove:**

- that the log holds every input a player sent after their last logged chain head: a player's receipts can prove it does not;
- that delays within the max input delay came from the network and not from the host;
- that bot and external inputs are honest: the server signs them;
- that the host did not use hidden state: the server sees everything, and can play with it or pass it to someone.

## Nostr events

Where a NIP already says what an event says, campfire uses it, so other Nostr clients, marketplaces and moderation tools read campfire's events: labels ([NIP-32](https://github.com/nostr-protocol/nips/blob/master/32.md), kind 1985) for reputation, reports ([NIP-56](https://github.com/nostr-protocol/nips/blob/master/56.md), kind 1984) for review requests, lists ([NIP-51](https://github.com/nostr-protocol/nips/blob/master/51.md)) for ban lists, classified listings ([NIP-99](https://github.com/nostr-protocol/nips/blob/master/99.md), kind 30402) for the marketplace, and Blossom ([NIP-B7](https://github.com/nostr-protocol/nips/blob/master/B7.md)) for files. Every other event is a campfire kind, chosen in the range NIP-01 gives its behavior: regular, replaceable, ephemeral or addressable. Campfire proposes its kinds as a NIP once the open network (milestone 2) runs them; until then the kinds and field lists are this spec's, and are fixed when milestone 2 builds each event.

| Event | Signed by | Content | Kind |
| --- | --- | --- | --- |
| Server listing | Server key | Address, TLS certificate hash, tick rate, engine release tag, region, protocol version, modes (fingerprints), capabilities, rules summary, prices | Campfire, addressable: one per server |
| Package announcement | Author key | Package id (author + name), version, fingerprint, license, download locations | Campfire, regular |
| Session log published | Server key | Session id, log and snapshot fingerprints, download locations, result | Campfire, regular |
| License | License key, with its delegation | Buyer main pubkey, package id, payment hash | Campfire, regular |
| Reputation statement | Any main or server key | Subject pubkey, session id, claim (e.g. paid out, log published, cheated), optional log fingerprint and tick range | NIP-32 label, in campfire's namespace |
| Review request | Any main or server key | Session id, reported pubkey, tick range, reason | NIP-56 report, with the session tags |
| Arbiter signature | Arbiter key | Session id, result, final state hash | Campfire, regular |
| Marketplace listing | Author key | Package id, price in sats, preview media, license terms | NIP-99 classified listing: one per package |
| Engine release | One release key; valid once k keys of the pinned set sign the same content | Release tag, protocol version, per-platform hashes of the unsigned build outputs, download locations | Campfire, regular |
| Release revocation | One release key; valid at the same threshold | Release tag, reason | Campfire, regular |
| Release key set | One release key of the current set; valid at the current threshold | New key set and threshold | Campfire, regular |
| Item export | Source server key | Export id, destination server key, item, owner's main pubkey, source session and tick, expiry | Campfire, regular ([Item export](#item-export)) |
| Ownership proof | Server key | Item id, owner's main pubkey, session and tick, expiry | Campfire, regular ([Item sale](#item-sale)) |
| Item listing | Owner's main key | Item id, its server key, price in sats, the server's ownership proof | NIP-99 classified listing: one per item |
| Sale receipt | Server key | Item id, seller and buyer main pubkeys, price, payment hash, preimage, session and tick | Campfire, regular ([Item sale](#item-sale)) |
| Ban list | Server or group key | Banned pubkeys, each with a reason and optional session id | NIP-51 set: one per signer |
| Server group | Group key | Member server pubkeys, shared rules summary | Campfire, addressable: one per group |

A license names the package id, not a fingerprint, so it covers future versions of the same hero or skin.

## Payment flows

The server talks to its own wallet over NWC ([NIP-47](https://github.com/nostr-protocol/nips/blob/master/47.md)). Hold invoices are not in core NIP-47 but in the [NWC hold invoice extension](https://github.com/nostr-wallet-connect/nwc/blob/main/03.md), so the server checks `get_info` and turns wager pools off when the wallet lacks it. The sim never waits on a payment: it only sees confirmed payment events, recorded as external inputs.

**Wager pool (and spectator pool)**

1. Waiting state: each player gives a payout Lightning address; the server creates a hold invoice for their stake (`make_hold_invoice`).
2. The player pays; the server receives `hold_invoice_accepted` and records a stake-locked event.
3. Result: the server settles all stakes (`settle_hold_invoice`) and pays winners, minus the host fee, to their payout addresses. Between settle and payout the host holds the whole pool. Above a stake the listing names, the result needs the signatures of the arbiters the listing names first: each replays the published log and co-signs only a result that verifies.
4. Aborted: the server cancels every hold invoice (`cancel_hold_invoice`); funds return to players automatically.

**Entry fee:** a normal invoice paid during waiting, recorded as an external input.

**Time-based and per-event charges:** the player's wallet grants the server an NWC connection with a spending budget; the server charges it as prices come due, each charge recorded. The player can revoke the connection at any time. No NWC specification defines budgets: the player's wallet enforces them, so the client shows which wallets it has tested.

**Rewards:** the server pays the player's Lightning address and records the payment.

### Item sale

A player sells a tradable item of a world to another player for sats, with no one holding the money in between: the world records the transfer, and only that record releases the secret that settles the payment, the hold invoice's preimage, which the [NWC hold invoice extension](https://github.com/nostr-wallet-connect/nwc/blob/main/03.md) lets a wallet take from the one who chose it.

1. **List.** The seller publishes an item listing with the price and an ownership proof the world's server signed, which expires; a buyer can check both without the game.
2. **Reserve.** The buyer asks the server to buy. The server checks that the item's type is tradable, that the seller owns it and that no other sale holds it, and records a reservation, so the item can be neither used nor moved; it picks a random preimage and gives its hash to the seller. A host fee, if any, is a separate invoice the buyer pays the host first.
3. **Invoice.** The seller's wallet makes a hold invoice for the price, with that hash; the buyer pays it, and the seller's wallet holds the payment, which it cannot settle without the preimage. The seller tells the server the payment is held.
4. **Transfer.** The server records the transfer, the item's owner the buyer's main key, and publishes a sale receipt that carries the preimage.
5. **Settle.** The seller's wallet settles with the preimage, from the receipt; the buyer's proof of payment is the same preimage.

- **Timeouts.** A reservation the seller does not confirm within a time the host sets ends by a recorded input, and the seller's wallet cancels its invoice, so the buyer's payment returns. The hold invoice's expiry is longer than the reservation by a margin the host sets, so a seller whose wallet is offline at step 5 can still settle.
- **Who can cheat whom.** A seller who claims a payment is held when it is not gives the item away for nothing: a lie that hurts only the seller. A buyer cannot take the item without the payment held first. A host that records no transfer but publishes the preimage pays the seller with the buyer's money and gives the buyer nothing, and a host that records the transfer but withholds the preimage gives the item and keeps the seller unpaid until expiry; either shows in the published log and the receipts, and is answered by reputation, as every host's dishonesty is.
- **Across worlds.** An item for sale in another world moves there first, by item export ([Item export](#item-export)), and is sold there.

**Failed payouts:** the server retries a failed payout for a period the host sets, then records it as unpaid in the result. The host still owes it; the player can publish a reputation statement with the claim "not paid".

## Item export

An item leaves one world for another by burn and attest, as Circle's [CCTP](https://chain.link/article/burn-and-mint-transfer) moves a token between chains: the transfer names its one destination, so no shared ledger is needed to keep the item from existing twice.

1. **Burn.** The player asks to export an item to a destination server, named by its key. The source server destroys the item through a recorded input, so its log shows the item gone, and signs an **item export** event: a unique export id, the destination server key, the item (its package id, its type and its state), the owner's main pubkey, the source session and tick, and an expiry.
2. **Redeem.** The player presents the event to the destination, which accepts it only when it names that server, comes from a server the host trusts, and has not expired, by the calendar time the destination records. The redemption is a recorded external input carrying the event: the item appears in the destination's world, and its export id joins the world's redeemed ids, which are state. A second redemption of the same id is refused, and the log proves it.
3. **Return.** An export not redeemed by its expiry returns to the source: the player redeems it there, after the expiry and a margin the host sets for clock skew, as at any destination.

What it proves: no honest server redeems one export twice, and no two honest servers both hold the item. What it does not prove: that the source server is honest. A source can mint items in its own world at will, so an item is worth the trust in the server that made it; the hosts that accept a source's items name it, and reputation tells them whom to trust.
