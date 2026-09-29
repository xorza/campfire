# Campfire

## Vision

An open-source (MIT/Apache-2.0) Rust engine for multiplayer games, plus a reference MOBA. Anyone can host a server, create content and set their own rules; the project runs nothing and ships defaults, not policy.

Three equal pillars must all ship: a reusable engine, a reference game good enough to attract players, and an open protocol for Bitcoin-native games.

Docs: [Engine Core](02-engine-core.md) · [Game Scripting](03-game-scripting.md) · [Game Kits](04-game-kits/00-overview.md) · [Protocol Spec](05-protocol-spec.md) · [Research Notes](06-research-notes.md)

## Guiding principles

- **Defaults, not enforcement.** Ownership checks, payments, character bans and rankings are options each host turns on.
- **Human trust over surveillance.** Anti-cheat lives on the server only; the game is built for people who trust each other.
- **Protocols over platforms.** Identity, server listings, reputation and the marketplace use Nostr and Lightning, so no one, including this project, is a chokepoint.
- **Licensing.** Code: MIT/Apache-2.0. Reference art and audio: CC-BY 4.0. Community content picks its own license.

## System overview

Three programs share one deterministic engine: the client, the game server, and a replay verifier anyone can run. Payments and ownership plug into the server as optional modules.

*Diagram: system architecture (3 programs, 3 optional protocols) — see the live doc.*

## Engine

The engine knows nothing about any particular genre; the MOBA is just the first game built on it. Genre features come as kits: MOBA first; FPS, MMO and battle royale later.

- **Deterministic:** the same inputs give the same result on every machine.
- **Configurable tick rate:** set by the host, up to 200 Hz or more on LAN, fixed for the whole match or session.
- **Platforms:** desktop only (Windows, Linux, macOS). The reference game is 3D with an isometric camera.
- **Tools:** map and content editors, dedicated server and replay verifier.

## Multiplayer model

One server is the single authority for each match or world. Peer-to-peer was rejected because every player would hold the full game state, and fog of war could be read.

- **Players see only what they should:** hidden information never reaches their machine.
- **Every match is verifiable:** the server keeps a session log of every signed input. Anyone can replay it on the tagged engine release it names and confirm the result, so a dishonest host can be caught.

**Anti-cheat** runs on the server only, never on players' machines: no kernel drivers, no scanning. Hosts get:

- every action is checked for being possible (range, cooldown, resources, line of sight);
- limits on action rate and on reaction times no human could achieve;
- tools to review match records, flag suspicious identities and ban them;
- reputation and stake caps for new identities, for hosts that use payments.

## Scale: from matches to large worlds

The same engine supports three sizes of game. One server always owns its whole match or world; there is no communication between servers for now.

| Size | Example games | Players (goal) | How results are verified |
| --- | --- | --- | --- |
| Match | MOBA, arena, duel | 2–20 | Replay the whole match |
| Battle | Battle royale, large siege | 20–200 | Replay the whole match |
| World | Persistent MMO-style world | 1,000+ on one server | Replay any period from a saved checkpoint |

Players move between worlds by leaving one server and joining another. Their identity comes with them; items and progress come with them only if the new server chooses to accept them.

## Scripting and modding

Characters, abilities, items, maps and whole game modes are content anyone can create. Packages hold scripts and data; genre features come from kits in engine releases.

Scripts are Rhai: sandboxed, deterministic and resource-limited, identical on server, client and verifier.

**Content packages** bundle logic, balance data, art and sound, signed by the author and identified by a unique fingerprint, so same-named content never conflicts. Hosts pin exact versions, so an author's update reaches players only when the host chooses.

## Decentralized server network

No central server list or account system: Nostr provides both.

- **Identity:** a player is a Nostr key, used on every server to own items and receive payouts. The main key never enters the game; it signs a short-lived key for each session.
- **Discovery:** servers publish listings (region, modes, content, rules, prices) that players browse.
- **Reputation:** players and servers publish signed statements after matches; each player chooses whom to trust. Servers may form groups that share bans, ratings and dispute handling.
- **Matchmaking** runs on each server or within a group of servers.

**The host decides:**

- which game modes, maps and characters are allowed, and whether character or skin ownership is enforced;
- tick rate and player limits;
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

**Wager pools.** The host sets stake rules (equal or free) and payout split (even or by stake); the default is equal and even. Aborted matches refund everyone; leavers lose their stake.

**Spectator bets** are off by default. The host sets when betting closes and the spectator delay; players in the match cannot bet.

**Who holds the money.** Stakes stay locked in the players' own payments until the match ends, and return automatically if it aborts. Later, independent arbiters can check the replay before paying out higher stakes.

**Legal.** Real-money features are regulated or banned in many countries; hosts are responsible for how they use them.

## Character and skin ownership

Owning a character or skin means the right to pick it on servers that enforce ownership, not the files: every player needs the files to see opponents.

- **Optional:** hosts decide; the reference settings enforce it for skins only.
- **Nostr licenses:** the creator signs a license after a Lightning payment. Other methods are not planned.
- **Marketplace:** creators list characters and skins on Nostr and sell them for sats; anyone can run a storefront.
- **Balance:** hosts decide which sold characters are allowed in ranked or paid games.
- **Creator revenue:** sales, plus an optional share of host fees for characters played.

## Sample game

The reference MOBA proves the engine and is the template people fork: small, readable and fun.

**First playable version:**

- 3v3 on a 2-lane map, 15–20 minute matches; 5v5 on 3 lanes later.
- 6 original heroes covering the classic roles, each with 3 abilities and an ultimate.
- About 20 items, one shop, gold and XP, last-hitting.
- Towers, one inhibitor-like structure per lane, a base core, one neutral objective, creep waves.
- Bots, so 2 humans can still play a full match.

**Design rule:** every reference hero and item is an ordinary package. If one needs something scripts cannot do, the scripting API is incomplete.

**Art:** stylized low-poly models, strong silhouettes and readable ability telegraphs.
