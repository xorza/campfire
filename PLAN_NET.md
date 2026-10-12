# Plan — network

Every planned piece of network work, in one place: the QUIC transport (`quic`), the connection protocol, replication and prediction (`net`), the hosting of a session in `net`, and milestone 2's open network. Open items only: an item goes when it is done, and a new network problem joins this file, not the issue log ([Issue log](AGENTS.md#issue-log)). The designs stay the source of truth: an item names what the code lacks against its design, or the question still open. Each step ends with the check chain passing for the crates it touches, and a stop for review.

## Order

The items below, in the order they are done; each line says why it stands where it does.

1. **Slice 3**, A3, A5 and A6, the next slice the user plays ([Workflow](AGENTS.md#workflow)); slice 2, in [PLAN.md](PLAN.md), goes on from S1 after it.
2. **N3**, limits for each address: after A3, whose spectators count among the seats it bounds by.
3. **R2**, the bench of the endpoint, then the cap of new connections it sets: on the same path as N3, the thread's new connections.
4. **N4**, UPnP behind a host firewall.
5. **R1**, the send queue: its rate model and its size.
6. **H1** to **H5**, each as a slice touches its code.
7. **R3** and **R4** as they show; then **P1** to **P4**, **P3** only as D14 of design 15 lets it.

## Ready

- **N3. Limits for each address** (`quic`, `net`). No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13).
- **N4. UPnP behind a host firewall** (`quic`). UPnP finds no router behind a host firewall that tracks connections when the router answers SSDP searches from a port other than 1900, as Omada's ER605 does: `igd-next` discovers by search alone, and the router's `NOTIFY` announcements, which the firewall lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

## Slice 3 — Replication audiences

The user sees their own gold, experience and cooldowns in the HUD, and another team's pools and cooldowns are on no client of theirs; a spectator watches the match, through all fog or one team's; a mode's `[visibility]` that shows every hero's items to all shows them; and a host that lets dead players watch the other team gives a dead hero's player that view until it respawns. The design is [Replication audiences](docs/design/14-replication-audiences.md); each step folds its rules into the design 04 file of each capability it changes, and into design 03's script state. The D numbers of this section are design 14's.

1. **A3. The mode's and the host's visibility, and the spectator** (`package`, `protocol`, `net`, `server`, `client`; D1, D4, D14, D15): `[visibility]` with `spectators`, `out` and `host`, and its load checks; the host's `--visibility` and `--spectators`, which the server checks against the mode's `host` list as it starts; the settings in the session's terms, the session id's `v3` and the log's tag `v4`, design 05; a spectator's link, which holds no slot, and its rooms: every room with `all`, the teams its message chooses with `teams`; the client's spectator, which predicts nothing, and its choice of team. Check: the load refuses each flaw of `[visibility]`, and the server each flaw of the host's settings; two sessions of other visibility settings have other ids; the leak test on the 3v3, with both settings, through the host's settings where the mode leaves them to it; a player's allies hold its gold when the mode adds allies to `resources`, and a spectator as the mode sets; a spectator holds the relations and the match's end.
2. **A5. Players out of play** (`capabilities`, `net`, `client`; D16): out of play while every avatar a player holds is dead, or while `ctx.set_out` holds it out, a field of its player entity; its link, which carries it, and the filters and rooms that follow it; design 08's script API, and its reference generated again. Check: in the leak test, a dead hero's player holds, with `out = "spectators"`, the other team's units through the spectators' fog and the kinds whose audiences hold spectators until its respawn, and none of them after it; with `team`, nothing more than its team's; a living player that a script holds out holds the same as a dead one.
3. **A6. Prediction** (`capabilities`, `net`; D5, D11, D12): `UnitStats` for every unit whose inputs a client holds; the prediction test. Check: a hero walks into an ally's aura, whose numbers come from its source's stats, with no correction; with `modifiers` hidden from enemies, an enemy aura corrects once.

What its steps fix, with the work each step takes beside its own:

- **A3.** Replication has no audience for each state type that a mode or its host sets, no spectator, and no view for a player out of play ([Replication audiences](docs/design/14-replication-audiences.md), D14 to D16). The design gives each capability's network data its own: all that see a unit, its team, or its owner, as the mode sets ([Combat](docs/design/04-capabilities/combat.md#network), [Progression](docs/design/04-capabilities/progression.md#network), [Production](docs/design/04-capabilities/production.md#network)). The code puts each replicated unit in the rooms of the teams that see it, hides each kind of an owner or a team from the other clients by a filter at the kind's default audience, which no mode sets. Two items are its cases: a player's supply and queues reach no client (Net), and a predicting client lacks a modifier source's stats (Stats). Every capability with network data meets it, and prediction lacks what it reads: a cause in the architecture, which the issue log's foundations would hold.
- **A6.** A held modifier whose numbers come from its source, as an aura's from the unit that carries it, resolves on a predicting client by `ParamSources`, which sees no unit the client only holds as the server sent it, and knows no such unit's stats, as only a unit's type and level replicate: where the server reads the source's stats, the client resolves other numbers and mispredicts the units it predicts.
- **A6, stats at the spawn.** A unit's stats derive in the first refresh after its spawn, not in the spawn, so a unit spawned in the middle of a stage has none until the stage's edge, and a client derives none.

## Decide


## Research

- **R1. The send queue** (`quic`). The send queue keeps `quinn-proto`'s 1 MiB: the delay a full queue adds is its size over the connection's sending rate, tens of seconds at the minimum window, and Lightyear queues a reliable message again while its last copy still waits there. No scenario measures the largest burst one client receives, as at its join, or the delay the queue adds: the harness's link models delay and loss, not a rate.
- **R2. The endpoint's benches** (`quic`). No bench measures the frame's two systems, the thread's batch, or the frame's waits for the mutex, nor a flood of handshakes from many addresses: what a handshake costs the thread under the mutex, and how long a batch of them holds the frame, which sets how many new connections a pass takes ([QUIC transport](docs/design/15-quic-transport.md), D14). No pass bounds its new connections.
- **R3. The thread's allocations** (`quic`). The thread allocates under the mutex at times `quinn-proto`'s random numbers choose: 8 times at a connection's first key update, within its first 1,000 packets, and once when the packet numbers a connection received first hold three ranges, as each packet number a sender skips leaves a gap until the ranges before it are acknowledged. A read also allocates a new arena when `RecvBatch`'s ring comes back to an arena that a message slice Lightyear keeps still holds.
- **R4. The LAN check's restart** (`lan-check`). The check starts bot 1 again as soon as the server logs bot 1's first order, while the other bot's order of the same stamp may still be on its way, and it fails an order that misses its stamp tick. In CI run 37523961750 on Ubuntu, attempt 1, the server logged bot 1's order stamped 20 at 20:13:55.960; bot 0 sent its own at 55.971, and the server logged it at 56.086, 115 ms later, in tick 21; the new bot 1 process started in that window and joined at 56.108. On a runner of 4 cores, the new process's start shares them with the server and bot 0.

## Session hosting in `net`

The [architecture audit](.notes/ARCHITECTURE-AUDIT.md)'s items that change `net`, from §5 and its single sources; their code stays in `net`, which hosts every session (design 02). The audiences' single source and the release are A3's.

- **H1. Server compositions.** The task pool, time, states and `SimServer` setup appears three times (`server/src/main.rs:234`, `net/src/local/local_server/mod.rs:327`, `net/src/harness/in_process_match/mod.rs:332`): `SimServer::app(tick, &ModePackages) -> App` holds it, and the server's edges to `capabilities` and `sim`, each used once, go.
- **H2. Restore glue.** The server's `Restore` and net's `PendingRestore` have the same fields and body: one type in net.
- **H3. Replay.** The loop of "take the checkpoint again, stop at a tick, run alone" is in `SimServer::restore_match` (`sim_server/mod.rs:351`) and `SessionDir::abort` (`session_dir/mod.rs:232`): `Session::replay_to(world, tick, on_boundary)`.
- **H4. Client edge.** `client` depends on `runner` for one rate check, which moves into `SimClient`'s setup.
- **H5. Log events.** The match's end is logged ad hoc with other fields on the server (`sim_server/mod.rs:663`) and the client (`sim_client/mod.rs:474`), and the client logs a death ad hoc (`sim_client/mod.rs:494`) though `UnitDied` exists: a `MatchEnded` event and `UnitDied` on both ends.

## Planned

- **P1. LAN listing** ([Protocol](docs/design/05-protocol-spec.md#connection), step 4): a LAN server announces its signed listing over mDNS; no code announces or reads one.
- **P2. Multiplayer pause** ([Sessions](docs/design/10-sessions.md#decisions), D6): pause and game speed work on the local server only; D6 leaves a pause of a multiplayer session, by the host's setting, to the Zero Hour skirmish.
- **P3. A puzzle before the handshake** ([QUIC transport](docs/design/15-quic-transport.md), D14): adaptive, dormant at effort 0, ordered by effort, memory-hard, in its own exchange on the server's port before QUIC, as QUIC's Retry carries no work; only once the flood's bench shows a reachable flood starves the server. Its puzzle needs a license Campfire can ship, which Arti's Equi-X, LGPL-3.0, is not.
- **P4. Milestone 2, the open network** ([Vision](docs/design/01-campfire-design.md#milestones), [Decentralized server network](docs/design/01-campfire-design.md#decentralized-server-network), [Nostr events](docs/design/05-protocol-spec.md#nostr-events)): Nostr listings and discovery, the `launcher` and its server browser with signed releases, packages over Blossom (`content`), reputation and ban lists (`identity`), each a planned module of [design 02](docs/design/02-engine-core.md#module-map).
