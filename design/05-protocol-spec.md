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

- A package is a file list: one `(path, size, SHA-256)` row per file, sorted by path bytes, paths in UTF-8 with `/` separators. Its fingerprint is the SHA-256 of the postcard-encoded list.
- Each file is a separate blob, so an update downloads only changed files and packages share identical assets.
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

**Session id:** the hash of the session's terms, which the server fixes when it opens the session, before any player joins: `BLAKE3("campfire/session-id/v1" ‖ server key ‖ u32 tick rate ‖ u64 max input delay ‖ u64 max input lead ‖ u32 max payload length ‖ u32 max inputs per tick ‖ seed commitment ‖ u64 release tag length ‖ release tag ‖ mode fingerprint ‖ u64 dependency count ‖ dependency fingerprints)`, integers little-endian. The release tag names the engine release the session runs on; the fingerprints name the mode package and its dependencies, in the order of their names in the mode's manifest. Every delegation and every chain-head signature names the id, so the players sign the terms, and a log cannot change them: a verifier computes the id from the header and refuses a delegation that names another. The seed commitment is fresh for every session, so no two sessions share an id.

**Session key delegation:** a Nostr event of a Campfire kind, signed by the main key, with tags for session pubkey, server pubkey, session id and expiry. It is a Nostr event because remote signers (NIP-46) sign only events. It is never published to relays; the session log header holds it verbatim, so a verifier can link every input to a player's identity. The main key never enters the game. The kind is 22710 for now, in the ephemeral range so a relay sent one by mistake does not keep it. Each term is one tag of one value: `session_key`, `server_key`, `session_id` and `seed_contribution` in lowercase hex, and NIP-40's `expiration` in Unix seconds; other tags and the content are ignored. The contribution is the player's random share of every segment's seed: the player chooses it when it delegates, after the server's seed commitment is fixed in the session id the delegation names, so the main key signs the contribution, and no one can change it or choose it with the seed in view. Only the contribution of the delegation in the header counts; a renewal's is ignored. The server checks the expiry against its clock when the player connects; a log has no clock, so a verifier does not.

**License key delegation:** the same shape, signed by the author key for a license key, with tags for the package ids it may license and an expiry. It lets a storefront issue licenses while the author is offline.

## Connection

The connection is part of the protocol, because it binds the Nostr identities to the transport; the game traffic inside it (`net`) is not.

1. The server uses a self-signed TLS certificate; the hash in its signed listing proves the client reached the server its key names.
2. The client connects over QUIC (WebTransport) and checks the certificate hash.
3. The server offers the session's terms and a random challenge, and sends the same offer again until the client answers. The client checks that the terms name the server key it expected, and its own release, mode packages and tick rate; then it delegates a fresh session key in that session and replies with its delegation and a session-key signature over `"campfire/connect/v1" ‖ challenge ‖ certificate hash it verified`. The server accepts only its own certificate hash and a delegation naming its own key, so a reply relayed from a connection to another server is useless there. This binds to the server certificate, like the `tls-server-end-point` channel binding of [RFC 5929](https://www.rfc-editor.org/rfc/rfc5929); a TLS exporter would be stronger, but Lightyear's WebTransport layer does not expose one. Players take slots in the order their replies are accepted, and the match starts when every slot is taken.
4. Home hosts forward a port or use UPnP. LAN servers also announce their signed listing over mDNS.
5. Against floods, cheap checks come first: QUIC address validation, limits on connections and packets per address, and a packet's size limits before its signature.

## Session log

```
SessionLog
  header
    protocol version
    terms: server pubkey, tick rate, max input delay and max input lead (ticks),
           max payload length, max inputs per player per tick, seed commitment,
           engine release tag, mode package fingerprint + dependency fingerprints
           (the session id is their hash, and is not written)
    players: session key delegation (which names the main pubkey and carries the seed contribution)
  segments[]
    checkpoint: tick, state hash, snapshot fingerprint, server signature
    ticks[]: packets[] logged before the tick ran: source,
             inputs[]: stamp tick, payload,
             session-key signature over the chain head after the last input
    seed reveal: server seed of this segment (added when the segment is published)
  result (optional): tick, result payload, final state hash, server signature
```

**File.** A session log file starts with the tag `campfire/session-log/v1`, which states its protocol version, and continues in postcard. Until checkpoints come, it holds one segment from tick 0: the header (the terms: server key, tick rate as a non-zero `u32`, max input delay and lead, max payload length, max inputs per tick, seed commitment, release tag as a string, mode fingerprint, and the dependency fingerprints as a list; then each player's delegation JSON, which carries the seed contribution), the `u64` count of ticks, the packets logged before each tick, the packets logged after the last tick, and the seed reveal as an option. Packets go as a `u32` count, then each as `u32 slot`, its inputs as a `u32` count and `u64 stamp tick, payload bytes` each, and the 64-byte signature. A reader checks every delegation and records every packet again, so it checks each chain link and signature, and accepts only the canonical encoding: postcard itself accepts an overlong varint, so a reader encodes what it decoded and compares the bytes. One log therefore has one file and one fingerprint. A verifier then replays the decoded log's ticks from what it holds, with no check done twice. It replays only a log of its own engine release, with the mode and dependencies it holds by the fingerprints the terms name, at a tick rate within the mode's range; any other log it refuses, each with its own error.

**Input sources**

| Source | Signed by | Payload |
| --- | --- | --- |
| Player | Session key | Commands, each in its capability's format (order, input frame, mode input) |
| Bot | Server key | Same format as a player input |
| External | Server key | Player connect or disconnect, character load, admin command, payment event, calendar time |

- A match is one segment starting at tick 0 from the initial state. A persistent world adds a checkpoint every few minutes.
- **Seed chain.** When it opens the session, the server fixes the server seed of every segment at once, as a one-way hash chain, the scheme of Lamport's one-time passwords and of provably fair games: it picks a random root `r` and a length `N`, the most segments the session may have, and segment `k`'s server seed is `s_k = C^(N−1−k)(r)`, where `C(x) = BLAKE3("campfire/seed-chain/v1" ‖ x)`. So `s_(N−1) = r`, and each seed is the hash of the next. The terms commit to `C(s_0)`. Revealing `s_k` reveals every earlier seed and no later one, and a verifier checks it alone: `k + 1` hashes lead it to the commitment.
- **Seed.** Each player's delegation carries a random seed contribution, which the player signs. Segment `k`'s seed is `BLAKE3("campfire/segment-seed/v1" ‖ u32 k ‖ s_k ‖ contributions in slot order)`. No one can steer any segment's seed: every server seed is fixed before any contribution and before any tick, a player knows no unrevealed server seed, and a signature covers every contribution. A server can still abort a session whose first seeds it dislikes, once it holds every delegation; an aborted session is what reputation and receipts answer. A verifier takes a seed only from the log: from the reveal, checked against the commitment. A seed is revealed only when its segment is published, because it predicts every hidden random outcome of the segment; the chain keeps the next segments' secret. A session that runs out of chain ends.
- Players who join later, and session key renewals, are added through a signed external input carrying the delegation. A renewal does not restart the player's input chain; the new key signs the chain heads from then on.
- Bot and external inputs are signed by the server key, one signature per input.
- A payload is a postcard list of commands, each the owning capability's index in the engine's fixed list and the command's bytes in that capability's format. The log keeps any payload within the max payload length; the sim ignores a command of a capability the mode did not declare, and one that does not decode.

**Player inputs**

- **Chain.** Each input of a player is linked to the one before it by a hash; the first links to the delegation's event id. Neither the link nor the seq is sent or logged: the client and the server each compute both from their own copy of the chain, and the signature over the head shows that the copies agree. A later signature therefore covers every earlier input, and a missing, reordered or altered input makes it fail. The hash is BLAKE3 of `"campfire/input-hash/v1" ‖ previous hash ‖ u32 slot ‖ u64 seq ‖ u64 stamp tick ‖ payload`, little-endian; seq counts a player's inputs from 0.
- **Signature.** The client signs the chain head once per packet, over `"campfire/input/v1" ‖ session id ‖ u32 player slot ‖ u64 seq ‖ chain head hash`, little-endian, where seq is the packet's last input's. The log keeps a packet whole or refuses it whole, with its signature, so every logged input is signed.
- **Limits.** The header fixes the max payload length and the max inputs a player may send before one tick, which together bound how fast a player can grow the log. The log refuses a packet over either, and a packet that would take the log past the 4 GiB its positions reach, as it refuses a broken link or signature.
- **Applied tick.** The server applies an input at `max(stamp tick, next tick)`. An input that would land more than the max input delay after its stamp is logged as late and not applied, so the chain stays whole. An input stamped more than the max input lead ahead of the next tick is logged as early and not applied, the same way: a predicting client stamps a few ticks ahead, and the server holds each input until its tick, so the lead bounds what a client can make it hold. The log groups inputs by the tick that was next when each arrived, so the applied tick, lateness and earliness follow from the log and are not stored: a verifier recomputes them and cannot be given wrong ones. The inputs applied in one tick take effect in slot order, then seq order, whatever order they arrived in, so the host cannot choose who acts first in a tick.
- **Receipts.** About once a second, the server signs `{session id, player slot, tick, highest seq received, chain head hash}` and sends it to the client, which keeps it. A receipt proves the server received every input up to that seq.

## Verification

1. Check every delegation, every chain link and chain-head signature, every bot and external input signature, and every applied tick against the delay rule.
2. Fetch the engine release named in the header, checked against engine release events from at least k keys of the pinned set, or build it from its tag.
3. Fetch packages by fingerprint and check the fingerprints.
4. Check the seed reveal against its commitment. Load the segment's checkpoint snapshot and check its hash.
5. Replay the inputs tick by tick.
6. Compare the state hash at the next checkpoint, or the final hash and result.

**What a successful check proves:** the result and every checkpoint follow from the logged inputs, on the named engine release and packages; every player input was signed by that player; no input is missing before a player's last logged chain head; and every applied tick keeps the delay rule.

**What it does not prove:**

- that the log holds every input a player sent after their last logged chain head: a player's receipts can prove it does not;
- that delays within the max input delay came from the network and not from the host;
- that bot and external inputs are honest: the server signs them;
- that the host did not use hidden state: the server sees everything, and can play with it or pass it to someone.

## Nostr events

| Event | Signed by | Content | Replaceable |
| --- | --- | --- | --- |
| Server listing | Server key | Address, TLS certificate hash, engine release tag, region, protocol version, modes (fingerprints), capabilities, rules summary, prices | Yes, one per server |
| Package announcement | Author key | Package id (author + name), version, fingerprint, license, download locations | No |
| Session log published | Server key | Session id, log and snapshot fingerprints, download locations, result | No |
| License | License key, with its delegation | Buyer main pubkey, package id, payment hash | No |
| Reputation statement | Any main or server key | Subject pubkey, session id, claim (e.g. paid out, log published, cheated), optional log fingerprint and tick range | No |
| Review request | Any main or server key | Session id, reported pubkey, tick range, reason | No |
| Arbiter signature | Arbiter key | Session id, result, final state hash | No |
| Marketplace listing | Author key | Package id, price in sats, preview media, license terms | Yes, one per package |
| Engine release | One release key; valid once k keys of the pinned set sign the same content | Release tag, protocol version, per-platform hashes of the unsigned build outputs, download locations | No |
| Release revocation | One release key; valid at the same threshold | Release tag, reason | No |
| Release key set | One release key of the current set; valid at the current threshold | New key set and threshold | No |
| Ban list | Server or group key | Banned pubkeys, each with a reason and optional session id | Yes, one per signer |
| Server group | Group key | Member server pubkeys, shared rules summary | Yes, one per group |

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

**Failed payouts:** the server retries a failed payout for a period the host sets, then records it as unpaid in the result. The host still owes it; the player can publish a reputation statement with the claim "not paid".

## Open questions

- [ ] Nostr event kind numbers, and whether to publish them as NIPs.
- [ ] Exact field lists for each payload.
- [ ] Item export between worlds: a server-signed attestation, and how the old server retires the item so it cannot exist twice.
