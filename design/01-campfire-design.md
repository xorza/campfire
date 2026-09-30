# Campfire

## Vision

An open-source (MIT/Apache-2.0) Rust engine for multiplayer games, plus a reference MOBA. Anyone can host a server, create content and set their own rules; the project runs nothing and ships defaults, not policy.

Three equal pillars must all ship: a reusable engine, a reference game good enough to attract players, and an open protocol for Bitcoin-native games.

Docs: [Engine Core](02-engine-core.md) · [Game Scripting](03-game-scripting.md) · [Capabilities](04-capabilities/00-overview.md) · [Protocol Spec](05-protocol-spec.md) · [Research Notes](06-research-notes.md) · [Reference MOBA](07-reference-moba.md) · [Script API](08-script-api.md) · [Determinism Core](09-determinism-core.md)

## Guiding principles

- **Defaults, not enforcement.** Ownership checks, payments, character bans and rankings are options each host turns on.
- **Human trust over surveillance.** Anti-cheat lives on the server only, and an open-source client cannot be trusted. So the game is built first for people who trust each other; a host who opens a server to strangers relies on server-side checks and reputation, and players there, above all in wagers, play at their own risk. The reference settings keep wagers and spectator bets off.
- **Protocols over platforms.** Identity, server listings, reputation and the marketplace use Nostr and Lightning, so no one, including this project, is a chokepoint.
- **Licensing.** Code: MIT/Apache-2.0. Reference art and audio: CC-BY 4.0. Community content picks its own license.

## Terms

| Term | Means |
| --- | --- |
| Match | A game with an end: waiting → running → ended |
| World | A persistent game that never ends |
| Session | One match or one world on one server, with one session id and one session log. A server restart continues the same session from its latest save |
| Segment | The part of a session log that starts at one checkpoint. A match is one segment |
| Team | Players who share vision. A mode has any number of teams |
| Params | Read-only values from data files (units, abilities, weapons) |
| Script state | Values scripts keep on units, players and the mode; part of the game state |

## System overview

Three programs share one deterministic engine: the client, the game server, and a replay verifier anyone can run. Payments and ownership plug into the server as optional modules.

A small launcher starts the right client: a server names only an engine release tag, and the launcher fetches that release only if enough of the release keys it trusts signed it (for example 2 of 3), and refuses revoked releases. The launcher itself is signed for each operating system.

*Diagram: system architecture (3 programs, 3 optional protocols) — see the live doc.*

## Engine

The engine knows nothing about any particular genre; the MOBA is just the first game built on it. It provides capabilities, one mechanism each (health and damage, units that take orders, a first-person character, fog of war), and a game declares the ones it needs: a MOBA, an FPS, or a mix that no genre names.

- **Deterministic:** the same inputs give the same result on every machine.
- **Configurable tick rate:** set by the host, up to 200 Hz or more on LAN, fixed for the whole session.
- **Platforms:** desktop only (Windows, Linux, macOS). The reference game is 3D with an isometric camera.
- **Tools:** map and content editors, dedicated server and replay verifier.

## Multiplayer model

One server is the single authority for each match or world. Peer-to-peer was rejected because every player would hold the full game state, and fog of war could be read.

- **Players see only what they should:** hidden information never reaches their machine.
- **Every result is verifiable:** the server keeps a session log of every signed input. Anyone can replay it on the tagged engine release it names and confirm that the result follows from the logged inputs. The log does not prove the host was fair: the host signs bot and external inputs, picks the tick each player input lands on, can drop inputs, and sees all hidden state. See [what verification proves](05-protocol-spec.md#verification).

**Anti-cheat** runs on the server only, never on players' machines: no kernel drivers, no scanning. Hosts get:

- every action is checked for being possible (range, cooldown, resources, line of sight);
- limits on action rate and on reaction times no human could achieve;
- tools to review match records, flag suspicious identities and ban them;
- reputation and stake caps for new identities, for hosts that use payments.

## Scale: from matches to large worlds

The same engine supports three sizes of game. One server always owns its whole match or world. Servers never connect to each other; what they share, they share as Nostr events.

| Size | Example games | Players (goal) | How results are verified |
| --- | --- | --- | --- |
| Match | MOBA, arena, duel | 2–20 | Replay the whole match |
| Battle | Battle royale, large siege | 20–200 | Replay the whole match |
| World | Persistent MMO-style world | 1,000+ on one server | Replay any period from a saved checkpoint, after a delay |

A world's log and checkpoints show hidden state that is still live, so the host publishes them only after a delay the host sets. A match publishes its log after it ends.

Players move between worlds by leaving one server and joining another. Their identity comes with them; items and progress come with them only if the new server chooses to accept them. How a server exports an item so it cannot exist twice is not designed yet.

## Scripting and modding

Characters, abilities, items, maps and whole game modes are content anyone can create. Packages hold scripts and data; mechanisms come from capabilities in engine releases, and a package may combine any of them.

Scripts are Rhai: sandboxed, deterministic and resource-limited. A game script gives the same result wherever the full sim runs: on the server, in the verifier, and in a client that plays back a published log.

**Content packages** bundle logic, balance data, art and sound, signed by the author and identified by a unique fingerprint, so same-named content never conflicts. Hosts pin exact versions, so an author's update reaches players only when the host chooses.

## Decentralized server network

No central server list or account system: Nostr provides both.

- **Identity:** a player is a Nostr key, used on every server to own items and receive payouts. The main key never enters the game; it signs a short-lived key for each session.
- **Discovery:** servers publish signed listings (region, modes, content, rules, prices) that players browse; LAN servers also announce themselves on the local network.
- **Reputation:** players and servers publish signed statements after matches; each player chooses whom to trust. Servers may form groups that share bans, ratings and dispute handling as Nostr events.
- **Matchmaking** runs on each server.

**The host decides:**

- which game modes, maps and characters are allowed, and whether character or skin ownership is enforced;
- tick rate, player limits, and the reconnect grace period;
- which payment models are on, their prices and caps, stake rules and spectator betting;
- ranking, matchmaking and extra plugins.

Settings can differ per map or mode, so a free casual map can run next to a paid one.

## Lightning payments

An optional module, off by default. Hosts turn on the models they want and set prices; a map or mode may declare its own prices within the host's limits.

| Model | Who pays whom | Example |
| --- | --- | --- |
| Wager pool | Losing team → winning team | Every player stakes, the winning team takes the pool |
| Spectator bets | Spectators → correct spectators | Viewers bet on the outcome in a separate pool |
| Entry fee | Player → host | Pay to join a ranked match or tournament |
| Time-based | Player → host | Pay per minute on a premium world server |
| Per event, charge | Player → host | Pay to respawn, fast travel or enter a boss arena |
| Per event, reward | Host → player | Bounty for a boss kill, tournament prize |

**Player consent.** Every price is shown before joining; nothing else can be charged. Time-based and per-event payments draw on a spending budget the player's wallet grants and can cancel at any time.

**Wager pools.** The host sets stake rules (equal or free) and payout split (even or by stake); the default is equal and even. A crashed match is restored and continues; it aborts only if its server is not back within the host's restore window, and an aborted match refunds everyone. Leavers lose their stake: a leaver sent a leave input or stayed disconnected longer than the host's grace period. The host decides whether bots may fill slots in wager matches; bot slots never stake, the listing shows the setting before anyone stakes, and the reference settings keep bots out of wagers.

**Spectator bets** are off by default. The host sets when betting closes and the spectator delay; players in the match cannot bet.

**Who holds the money.** Until the match ends, each stake is a locked payment that the host cannot take; it returns automatically if the match aborts. At the end the host settles every stake, holds the whole pool, and pays the winners from it, so at payout the players trust the host. Later, independent arbiters can check the replay before higher stakes settle.

**Legal.** Real-money features are regulated or banned in many countries; hosts are responsible for how they use them.

## Character and skin ownership

Owning a character or skin means the right to pick it on servers that enforce ownership, not the files: every player needs the files to see opponents.

- **Optional:** hosts decide; the reference settings enforce it for skins only.
- **Nostr licenses:** after a Lightning payment, a license key the creator delegated to a storefront signs a license, so the creator need not be online. A license belongs to the buyer's main key; losing that key loses its licenses. Other methods are not planned.
- **Marketplace:** creators list characters and skins on Nostr and sell them for sats; anyone can run a storefront.
- **Balance:** hosts decide which sold characters are allowed in ranked or paid games.
- **Creator revenue:** sales, plus a share of host fees for characters played that a host may choose to pay; the protocol cannot enforce it.

## Sample game

The reference MOBA proves the engine and is the template people fork: small, readable and fun.

**First playable version:**

- 3v3 on a 2-lane map, 15–20 minute matches; 5v5 on 3 lanes later.
- 6 original heroes covering the classic roles, each with 3 abilities and an ultimate; see [Reference MOBA](07-reference-moba.md).
- Player spells, random crits, brush, wards and stealth.
- About 20 items, one shop, gold and XP, last-hitting.
- Towers, one inhibitor-like structure per lane, a base core, one neutral objective, creep waves.
- Bots, so 2 humans can still play a full match.

**Design rule:** every reference hero and item is an ordinary package. If one needs something scripts cannot do, the scripting API is incomplete.

**Art:** stylized low-poly models, strong silhouettes and readable ability telegraphs.

## Milestones

All three pillars ship in 1.0; they arrive in this order.

1. **Playable on LAN.** First the determinism core, `det-ci` on every OS, and the prototype that proves the sim runs the same inside Lightyear and in a bare verifier. Then the 3v3 MOBA with bots on LAN or a local server, verified replays and crash restore. Players use local Nostr key files through the final delegation, handshake and session log formats; no relays, listings, launcher or payments.
2. **Open network.** Nostr listings, packages over Blossom, reputation, ban lists, and the launcher with signed releases.
3. **Payments.** The optional `payments` module: entry fees first, then wager pools, then the rest.
4. **More capabilities.** `character` and `hitscan` for first-person games, then `persistence` and `physics` for persistent worlds and battle royale.
