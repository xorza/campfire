# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Lightyear in the tree

The slice of [Lightyear in the tree](docs/design/12-lightyear-in-tree.md): the player hosts a LAN match and plays a singleplayer match as before, every link on Campfire's own network stack, copied from the fork's `main` into `source/crates/network/`, over its own UDP. The systems it changes: the new `net-*` crates, `common`, `math`, `store`, `codec`, `sim`, `runner`, `protocol`, `rules`, `net`, `quic` (deleted), `harness`, `client`, `server`, `lan-check`, every crate on Bevy, the workspace's `Cargo.toml` and `clippy.toml`, CI, and the design documents 02, 08, 10, 11 and `04-capabilities/vision.md`. Each batch ends with `cargo drive gate` passing; a golden that moves names its batch and its cause.

### 1. The stack in the tree

The new crates `campfire-net-common` (the fork's `common` but its `Worker` and `ErrorReport`), `-net-core`, `-net-bytes` (`serde`), `-net-timeline`, `-net-payload`, `-net-emulator` (`sim`), `-net-link`, `-net-connection`, `-net-packets` (`session`), `-net-udp`, `-net-endpoint`, `-net-stepper`, `-net-channels`, `-net-transport`, `-net-messages`, `-net-sync`, `-net-replication`, `-net-interpolation`, `-net-prediction`, `-net-port-map`, `-net-despawn` (the replication crates' one despawn) and `-net-stack` (the facade); `store` (`Worker`, `Parallel`, `SharedPort` for the port map); `rules`; the workspace's `Cargo.toml` and `clippy.toml`; the design `02-engine-core.md` (the crates table, Platform) and `13-net-stack/`; `docs/research/`. It stands first, as every later batch changes code that must already pass Campfire's rules; no Campfire crate depends on the stack yet. It costs one batch of two Bevys in the build: the stack's crates name Bevy 0.20 and Replicon 0.45 in their own manifests until batch 3 moves the workspace. Builds: a module is one entry of one list; each crate depends only on lower layers.

- **Check:** the stack's tests pass under `cargo drive test`, each within its bound; `cargo drive gate` passes; no Campfire crate names a `net-*` crate.

### 2. The stack on Campfire's base crates

`net-core`, `net-emulator`, `net-packets`, `net-udp`, `net-port-map` and the stack crates that use them; `common` (`TickRate`, `ChannelBinding`), `math` (`SplitMix64`), `sim` and `runner` (`TickRate`). It stands second, as the merges change the types Campfire's code will name, so batch 3 ports onto the final ones. Builds: one word, one meaning.

- [ ] source/crates/network/net-core/src/tick_time/ — tick spans are `fixed` (One number type). → `TickDelta`, the non-negative span and the overstep hold `Num`; the emulator's probabilities are `Num`s, drawn against 24 bits as `Rng::chance` draws; `fixed` leaves the lockfile. The fork's tests of spans pin the same values, the step of 2⁻²⁴ tick shown.
- [ ] source/crates/sim/sim/src/tick_rate/ — two `TickRate`s, of two roundings (One tick rate). → One `TickRate` in `campfire-common`, ticks over whole seconds, a tick's length to the nearest nanosecond; the sim, the session's terms and the timelines read it; a test runs half of 21 Hz for 2 s and counts 21 ticks.
- [ ] source/crates/network/net-common/src/split_mix64.rs — a second `SplitMix64` and a second `ChannelBinding` (One home for each shared helper). → `SplitMix64` is `campfire-math`'s, out of its `internals`; `ChannelBinding` is `campfire-common`'s.
- **Check:** the stack's tests pass; the sim's conversions of milliseconds to ticks are unchanged; `cargo drive gate` passes.

### 3. Campfire on the stack

Every crate on Bevy; `net`, `client`, `server`, `harness`, `lan-check`; `quic`, deleted; the workspace's `Cargo.toml` (Bevy 0.20, Replicon 0.45, no `[patch]`), `clippy.toml` (the Lightyear bans go) and drive table; CI. It stands third, and is one commit, as every crate above `net` depends on Lightyear on Bevy 0.19 and no fork commit holds that API on Bevy 0.20 (research, Its Bevy). Builds: one rendering pipeline is untouched; a capability asks another through a contract below both.

- [ ] source/Cargo.toml:169 — Campfire is on Bevy 0.19 and Replicon 0.44 with a patch (Bevy 0.20 and Replicon 0.45 from crates.io). → Bevy 0.20 and Replicon 0.45.2 for every crate, the stack's own Bevy lines and the `[patch]` section gone, each crate ported to Bevy 0.20.
- [ ] source/crates/network/net/src/net_protocol/ — `WireCodec` and the channel settings of Lightyear 0.30 (One encoding of a message and a component). → The protocol registers its channels with `ChannelConfig`, its messages and components with the stack's registration, which encodes through `codec`; `WireCodec` and the Lightyear bans of `clippy.toml` go.
- [ ] source/crates/network/quic/ — links over QUIC (The links run on the stack's UDP; Campfire keeps what is its own at the endpoint). → A server and a client link through `net-udp`'s endpoints; the server's secret derives from its own key, the ALPN is `campfire`, the connect answer signs the link's `ChannelBinding`, the admission holds the seats. Everything that exists only for QUIC goes: the `quic` crate, its benches and its allocation tests; `campfire-quic` and `quinn-proto` in the workspace's `Cargo.toml`, `quinn-udp` taken to the stack's version; the drive table's and CI's mentions of it; `alloc_trace`'s fixture paths into it; the issue log's `Quic` items and Foundations' F1. What is Campfire's own stays, its words no longer QUIC's: `TlsKey`, which the stack's raw key replaces if they are one key, `ChannelBinding`, `CloseCode` and the server's key file.
- [ ] source/crates/network/net/src/local/ — singleplayer links over `CrossbeamIo` (One in-process link; The pace sets the tick rate). → The local server and its client link through the emulator on the OS clock with a perfect model; the pace sets the match's rate times the speed through `SetTickRate`.
- [ ] source/crates/test/harness/src/in_process_match/ — the harness models links with its own `DelayLine` and pins Lightyear's round trip (The harness's time is Bevy's). → Each app's time follows `TimeUpdateStrategy` and the stack's `Frame` clock; links follow the stack's `LinkModel`; `DelayLine`, the harness's `LinkModel` and the round-trip pin go.
- [ ] source/crates/apps/ — the client, the server and the LAN check name Lightyear 0.30 (The links run on the stack's UDP). → Each names the stack's crates; the LAN check runs over UDP; the issue log's items of the ping manager and the fragment count, which this batch takes, go once its checks pass.
- **Check:** the harness's goldens, the 3v3 scripted match's among them, hold, or each that moves names its cause; the LAN check passes on Linux, macOS and Windows and the cross-verify verifies each platform's match; a singleplayer match pauses and runs at each speed with its prediction in step; `cargo drive gate` passes.

### 4. The fold

`docs`: `02-engine-core.md` (Networking, the crates table), `05-protocol-spec.md`, `08-sessions.md`, `10-replication-audiences.md`, `04-capabilities/vision.md`, `11-quic-transport.md` and `docs/research/quic.md` (deleted). It stands last, as a slice ends when the user plays it.

- [ ] docs/design/05-protocol-spec.md:51 — the connect flow says a client connects over QUIC, which a third party implements with any QUIC library (The links run on the stack's UDP). → The spec names the stack's UDP: its handshake packets, its header and its seal, with their byte layouts, as a third party now implements Campfire's own protocol; the Retry and the limits per address keep their place in the flow.
- [ ] docs/research/quic.md — facts of a transport no decision takes any more. → Deleted, but each fact a decision of `13-net-stack/udp.md` cites, as `quinn-udp`'s socket, which moves into the stack's UDP research.

- [ ] docs/design/12-lightyear-in-tree.md — the slice's decisions stand in a document of its own (A slice's proposal is temporary). → Once the user plays a LAN match and a singleplayer match on the stack, each lasting decision folds into `13-net-stack/` or the document of the system it governs, the decisions of `11-quic-transport.md` that are Campfire's own into `08-sessions.md`, "Lightyear" and "QUIC" become the stack's names in each document and in each citation of the code, and designs 11 and 12 are deleted.

## Steps

**T1. The toolchain back to stable.** Touches `source/rust-toolchain.toml`. When Rust 1.100 is released, on 2026-11-12, the pin moves from the dated beta to stable 1.100, as its comment says.
- **Check:** `cargo drive gate` passes on the stable toolchain.

## Foundations

- **F1. No load scenario for the QUIC thread.** Cause: the harness's links model delay and loss, not a rate, and no bench drives the thread with many connections at once. Limits: what a burst, a flood of handshakes or a full send queue costs the thread and the frame is not known, so no pass bounds its work. Items: the four Quic items of the issue log.
