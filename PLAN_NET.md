# Plan — network

Every planned piece of network work, in one place: the QUIC transport (`quic`), the connection protocol, replication and prediction (`net`), the hosting of a session in `net`, and milestone 2's open network. Open items only: an item goes when it is done, and a new network problem joins this file, not the issue log ([Issue log](AGENTS.md#issue-log)). The designs stay the source of truth: an item names what the code lacks against its design, or the question still open. Each step ends with the check chain passing for the crates it touches, and a stop for review.

## Order

The items below, in the order they are done; each line says why it stands where it does.

1. **Slice 3**, whose steps are done: it ends as the user plays it ([Workflow](AGENTS.md#workflow)); slice 2, in [PLAN.md](PLAN.md), goes on from S1 after it.
2. **N3**, limits for each address: after A3, whose spectators count among the seats it bounds by.
3. **R2**, the bench of the endpoint, then the cap of new connections it sets: on the same path as N3, the thread's new connections.
4. **N4**, UPnP behind a host firewall.
5. **R1**, the send queue: its rate model and its size.
6. **H1** to **H5**, each as a slice touches its code.
7. **R3**, **R4** and **R6** as they show; then **P1** to **P4**, **P3** only as D14 of design 15 lets it.

## Ready

- **N3. Limits for each address** (`quic`, `net`). No limit holds live connections or new handshakes for each address, where design 15's D13 bounds both by the session's seats for each IPv4 address or IPv6 /64, and `net` gives the endpoint no seat count ([QUIC transport](docs/design/15-quic-transport.md), D13).
- **N4. UPnP behind a host firewall** (`quic`). UPnP finds no router behind a host firewall that tracks connections when the router answers SSDP searches from a port other than 1900, as Omada's ER605 does: `igd-next` discovers by search alone, and the router's `NOTIFY` announcements, which the firewall lets in, go unread ([QUIC transport](docs/design/15-quic-transport.md), D12).

## Slice 3 — Replication audiences

The user sees their own gold, experience and cooldowns in the HUD, and another team's pools and cooldowns are on no client of theirs; a spectator watches the match, through all fog or one team's; a mode's `[visibility]` that shows every hero's items to all shows them; and a host that lets dead players watch the other team gives a dead hero's player that view until it respawns. The design is [Replication audiences](docs/design/14-replication-audiences.md); each step folds its rules into the design 04 file of each capability it changes, and into design 03's script state. The D numbers of this section are design 14's.

Its steps are done; the slice ends as the user plays it in the client.

## Decide


## Research

- **R1. The send queue** (`quic`). The send queue keeps `quinn-proto`'s 1 MiB: the delay a full queue adds is its size over the connection's sending rate, tens of seconds at the minimum window, and Lightyear queues a reliable message again while its last copy still waits there. No scenario measures the largest burst one client receives, as at its join, or the delay the queue adds: the harness's link models delay and loss, not a rate.
- **R2. The endpoint's benches** (`quic`). No bench measures the frame's two systems, the thread's batch, or the frame's waits for the mutex, nor a flood of handshakes from many addresses: what a handshake costs the thread under the mutex, and how long a batch of them holds the frame, which sets how many new connections a pass takes ([QUIC transport](docs/design/15-quic-transport.md), D14). No pass bounds its new connections.
- **R3. The thread's allocations** (`quic`). The thread allocates under the mutex at times `quinn-proto`'s random numbers choose: 8 times at a connection's first key update, within its first 1,000 packets, and once when the packet numbers a connection received first hold three ranges, as each packet number a sender skips leaves a gap until the ranges before it are acknowledged. A read also allocates a new arena when `RecvBatch`'s ring comes back to an arena that a message slice Lightyear keeps still holds.
- **R4. The LAN check's restart** (`lan-check`). The check starts bot 1 again as soon as the server logs bot 1's first order, while the other bot's order of the same stamp may still be on its way, and it fails an order that misses its stamp tick. In CI run 37523961750 on Ubuntu, attempt 1, the server logged bot 1's order stamped 20 at 20:13:55.960; bot 0 sent its own at 55.971, and the server logged it at 56.086, 115 ms later, in tick 21; the new bot 1 process started in that window and joined at 56.108. On a runner of 4 cores, the new process's start shares them with the server and bot 0.
- **R6. The exclusive systems a client predicts with** (`capabilities`). D12's test, `release::tests::prediction_reads`, reads no access of an exclusive system, as Bevy gives one none, so it does not check what eight systems of a predicting client read: `Channels::predict_channels`, `Attacks::attack_events`, `DamagePass::run`, `ModifierIntervals::run_intervals`, `DeliveryHooks::end`, `Construction::start_builds`, `GatherLoop::run` and `Trains::finish_trains`, each through `QueryState`, `SystemState` or the world ([Replication audiences](docs/design/14-replication-audiences.md), D12). The test names them in `EXCLUSIVE`, and fails on another.

## Session hosting in `net`

The architecture audit's items that change `net`: the session's host and its single sources; their code stays in `net`, which hosts every session (design 02). The audiences' single source and the release are A3's.

- **H1. Server compositions.** The task pool, time, states and `SimServer` setup appears three times (`server/src/main.rs:234`, `net/src/local/local_server/mod.rs:327`, `net/src/harness/in_process_match/mod.rs:332`): `SimServer::app(tick, &ModePackages) -> App` holds it, and the server's edges to `capabilities` and `sim`, each used once, go.
- **H2. Restore glue.** The server's `Restore` and net's `PendingRestore` have the same fields and body: one type in net.
- **H3. Replay.** The loop of "take the checkpoint again, stop at a tick, run alone" is in `SimServerWorld::restore_match` (`sim_server/sim_server_world/mod.rs`) and `SessionDir::abort` (`session_dir/mod.rs`): `SessionWorld::replay_to(tick, on_boundary)`.
- **H4. Client edge.** `client` depends on `runner` for one rate check, which moves into `SimClient`'s setup.
- **H5. Log events.** The match's end is logged ad hoc with other fields on the server (`sim_server/mod.rs:663`) and the client (`sim_client/mod.rs:474`), and the client logs a death ad hoc (`sim_client/mod.rs:494`) though `UnitDied` exists: a `MatchEnded` event and `UnitDied` on both ends.

## Planned

- **P1. LAN listing** ([Protocol](docs/design/05-protocol-spec.md#connection), step 4): a LAN server announces its signed listing over mDNS; no code announces or reads one.
- **P2. Multiplayer pause** ([Sessions](docs/design/10-sessions.md#decisions), D6): pause and game speed work on the local server only; D6 leaves a pause of a multiplayer session, by the host's setting, to the Zero Hour skirmish.
- **P3. A puzzle before the handshake** ([QUIC transport](docs/design/15-quic-transport.md), D14): adaptive, dormant at effort 0, ordered by effort, memory-hard, in its own exchange on the server's port before QUIC, as QUIC's Retry carries no work; only once the flood's bench shows a reachable flood starves the server. Its puzzle needs a license Campfire can ship, which Arti's Equi-X, LGPL-3.0, is not.
- **P4. Milestone 2, the open network** ([Vision](docs/design/01-campfire-design.md#milestones), [Decentralized server network](docs/design/01-campfire-design.md#decentralized-server-network), [Nostr events](docs/design/05-protocol-spec.md#nostr-events)): Nostr listings and discovery, the `launcher` and its server browser with signed releases, packages over Blossom (`content`), reputation and ban lists (`identity`), each a planned module of [design 02](docs/design/02-engine-core.md#module-map).
