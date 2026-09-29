# Campfire — Protocol Spec

## Scope and versioning

Everything a third party needs to build a compatible client, verifier, storefront or wallet integration:

- keys and signatures;
- the session log (what a replay needs);
- Nostr events for listings, packages, licenses, reputation and the marketplace;
- payment flows.

Not in the protocol: state replication between client and server (`net`), which is internal because both always run the same engine release.

The protocol has its own version, separate from engine releases. Every session log and event states the protocol version it uses.

## Encoding and cryptography

| Item | Rule |
| --- | --- |
| Binary encoding | `postcard` 1.x (stable wire format) |
| Numbers in game data | 32.32 fixed-point, stored as raw `i64` |
| Content fingerprints, state hashes | BLAKE3, 32 bytes |
| Signatures | Schnorr over secp256k1, as in Nostr |
| Nostr events | Standard Nostr JSON; payloads inside are defined here |

## Keys

| Key | Owner | Signs |
| --- | --- | --- |
| Main key | Player (Nostr identity; ideally held by a remote signer) | Session key delegations, licenses held, reputation statements |
| Session key | Player's client, one per session, short-lived | Every input |
| Server key | Host (Nostr identity) | Listings, checkpoints, external inputs, results |
| Author key | Content creator | Package announcements, licenses issued |

**Session key delegation:** the main key signs `{session pubkey, server pubkey, session id, expiry}`. The delegation goes into the session log header, so a verifier can link every input to a player's identity. The main key never enters the game.

## Session log

```
SessionLog
  header
    protocol version, engine release tag
    session id, server pubkey
    mode package fingerprint + dependency fingerprints
    host settings hash, tick rate, backends, kits
    players: main pubkey + session key delegation
  segments[]
    checkpoint: tick, state hash, snapshot fingerprint, server signature
    inputs[]: tick, source, payload, signature
  result (optional): tick, result payload, final state hash, server signature
```

**Input sources**

| Source | Signed by | Payload |
| --- | --- | --- |
| Player | Session key | Kit-defined format (MOBA: order; FPS: input frame) |
| Bot | Server key | Same format as a player input |
| External | Server key | Character load, admin command, payment event, calendar time |

- A match is one segment starting at tick 0 from the initial state. A persistent world adds a checkpoint every few minutes.
- Players who join later are added through a signed external input carrying their delegation.
- Unknown payload formats are rejected.

## Verification

1. Check every delegation and input signature.
2. Build or fetch the engine release named in the header (git tag).
3. Fetch packages by fingerprint and check the fingerprints.
4. Load the segment's checkpoint snapshot and check its hash.
5. Replay the inputs tick by tick.
6. Compare the state hash at the next checkpoint, or the final hash and result.

## Nostr events

| Event | Signed by | Content | Replaceable |
| --- | --- | --- | --- |
| Server listing | Server key | Address, region, protocol version, modes (fingerprints), kits, rules summary, prices | Yes, one per server |
| Package announcement | Author key | Package id (author + name), version, fingerprint, license, download locations | No |
| Session log published | Server key | Session id, log fingerprint, download locations, result | No |
| License | Author key | Buyer main pubkey, package id, payment hash | No |
| Reputation statement | Any main or server key | Subject pubkey, session id, claim (e.g. paid out, log published), optional log fingerprint | No |
| Marketplace listing | Author key | Package id, price in sats, preview media, license terms | Yes, one per package |

A license names the package id, not a fingerprint, so it covers future versions of the same hero or skin.

## Payment flows

The server talks to its own wallet over NWC (NIP-47). The sim never waits on a payment: it only sees confirmed payment events, recorded as external inputs.

**Wager pool (and spectator pool)**

1. Waiting state: each player gives a payout Lightning address; the server creates a hold invoice for their stake (`make_hold_invoice`).
2. The player pays; the server receives `hold_invoice_accepted` and records a stake-locked event.
3. Result: the server settles all stakes (`settle_hold_invoice`) and pays winners, minus the host fee, to their payout addresses.
4. Aborted: the server cancels every hold invoice (`cancel_hold_invoice`); funds return to players automatically.

**Entry fee:** a normal invoice paid during waiting, recorded as an external input.

**Time-based and per-event charges:** the player's wallet grants the server an NWC connection with a spending budget; the server charges it as prices come due, each charge recorded. The player can revoke the connection at any time.

**Rewards:** the server pays the player's Lightning address and records the payment.

## Open questions

- [ ] Nostr event kind numbers, and whether to publish them as NIPs.
- [ ] Exact field lists for each payload.
