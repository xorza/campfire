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
| Numbers in game data | 32.32 fixed-point, stored as raw `i64` |
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
| Main key | Player (Nostr identity; ideally held by a remote signer) | Session key delegations, licenses held, reputation statements |
| Session key | Player's client, one per session, short-lived; a long session renews it with a new delegation before it expires | Chain heads of the player's inputs |
| Server key | Host (Nostr identity) | Listings, checkpoints, bot and external inputs, input receipts, results |
| Author key | Content creator | Package announcements, license key delegations |
| License key | A storefront or service the author delegates to | Licenses issued |
| Group key | A group of hosts | Group membership, shared ban lists |
| Release keys | The project's maintainers (or any fork's), kept offline; a launcher pins the set and its threshold k of n | Engine release, release revocation and key-set change events |

**Session key delegation:** a Nostr event of a Campfire kind, signed by the main key, with tags for session pubkey, server pubkey, session id and expiry. It is a Nostr event because remote signers (NIP-46) sign only events. It is never published to relays; the session log header holds it verbatim, so a verifier can link every input to a player's identity. The main key never enters the game.

**License key delegation:** the same shape, signed by the author key for a license key, with tags for the package ids it may license and an expiry. It lets a storefront issue licenses while the author is offline.

## Connection

The connection is part of the protocol, because it binds the Nostr identities to the transport; the game traffic inside it (`net`) is not.

1. The server uses a self-signed TLS certificate; the hash in its signed listing proves the client reached the server its key names.
2. The client connects over QUIC (WebTransport) and checks the certificate hash.
3. The server sends a random challenge. The client replies with its delegation and a session-key signature over `"campfire/connect/v1" ‖ challenge ‖ certificate hash it verified`. The server accepts only its own certificate hash and a delegation naming its own key, so a reply relayed from a connection to another server is useless there. This binds to the server certificate, like the `tls-server-end-point` channel binding of [RFC 5929](https://www.rfc-editor.org/rfc/rfc5929); a TLS exporter would be stronger, but Lightyear's WebTransport layer does not expose one.
4. Home hosts forward a port or use UPnP. LAN servers also announce their signed listing over mDNS.

## Session log

```
SessionLog
  header
    protocol version, engine release tag
    session id, server pubkey
    mode package fingerprint + dependency fingerprints
    host settings hash, tick rate, max input delay (ticks), backends, kits
    players: main pubkey + session key delegation + seed contribution
  segments[]
    checkpoint: tick, state hash, snapshot fingerprint,
                seed commitment for this segment, server signature
    inputs[]: source, seq, stamp tick, applied tick or late, previous hash, payload
    chain heads[]: player, seq, session-key signature
    seed reveal: server seed of this segment (added when the segment is published)
  result (optional): tick, result payload, final state hash, server signature
```

**Input sources**

| Source | Signed by | Payload |
| --- | --- | --- |
| Player | Session key | Kit-defined format (MOBA: order; FPS: input frame) |
| Bot | Server key | Same format as a player input |
| External | Server key | Player connect or disconnect, character load, admin command, payment event, calendar time |

- A match is one segment starting at tick 0 from the initial state. A persistent world adds a checkpoint every few minutes.
- **Seed.** In waiting, the server commits `BLAKE3(server seed)`; each player then sends a random seed contribution. The first segment's seed is `BLAKE3(server seed ‖ contributions in slot order)`. Each later checkpoint commits the next segment's server seed before that segment starts, and that seed is the segment's seed. A seed is revealed only when its segment is published, because it predicts every hidden random outcome.
- Players who join later, and session key renewals, are added through a signed external input carrying the delegation. A renewal does not restart the player's input chain; the new key signs the chain heads from then on.
- Bot and external inputs are signed by the server key, one signature per input.
- Unknown payload formats are rejected.

**Player inputs**

- **Chain.** Each input of a player carries the hash of that player's previous input; the first carries the delegation hash. A later signature therefore covers every earlier input, and a dropped input breaks the chain.
- **Signature.** The client signs the chain head once per packet, over `"campfire/input/v1" ‖ session id ‖ player slot ‖ seq ‖ chain head hash`.
- **Applied tick.** The server applies an input at `max(stamp tick, next tick)`. An input that would land more than the max input delay after its stamp is logged as late and not applied, so the chain stays whole.
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
| Server listing | Server key | Address, TLS certificate hash, engine release tag, region, protocol version, modes (fingerprints), kits, rules summary, prices | Yes, one per server |
| Package announcement | Author key | Package id (author + name), version, fingerprint, license, download locations | No |
| Session log published | Server key | Session id, log and snapshot fingerprints, download locations, result | No |
| License | License key, with its delegation | Buyer main pubkey, package id, payment hash | No |
| Reputation statement | Any main or server key | Subject pubkey, session id, claim (e.g. paid out, log published), optional log fingerprint | No |
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
3. Result: the server settles all stakes (`settle_hold_invoice`) and pays winners, minus the host fee, to their payout addresses. Between settle and payout the host holds the whole pool.
4. Aborted: the server cancels every hold invoice (`cancel_hold_invoice`); funds return to players automatically.

**Entry fee:** a normal invoice paid during waiting, recorded as an external input.

**Time-based and per-event charges:** the player's wallet grants the server an NWC connection with a spending budget; the server charges it as prices come due, each charge recorded. The player can revoke the connection at any time. No NWC specification defines budgets: the player's wallet enforces them, so the client shows which wallets it has tested.

**Rewards:** the server pays the player's Lightning address and records the payment.

**Failed payouts:** the server retries a failed payout for a period the host sets, then records it as unpaid in the result. The host still owes it; the player can publish a reputation statement with the claim "not paid".

## Open questions

- [ ] Nostr event kind numbers, and whether to publish them as NIPs.
- [ ] Exact field lists for each payload.
- [ ] Item export between worlds: a server-signed attestation, and how the old server retires the item so it cannot exist twice.
