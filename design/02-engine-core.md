# Campfire — Engine Core

## Module map

*Diagram: module map (apps, services, deterministic core with capabilities) — see the live doc.*

Each layer uses the layers below it.

## Modules

| Module | Does |
| --- | --- |
| `math` | Fixed-point numbers, 3D vectors, trig, counter-based RNG, and the values both sides share: segment seed, player slot, hex |
| `protocol` | Session log format (see Protocol Spec) |
| `sim` | Deterministic state and systems on `bevy_ecs`; no genre code |
| `script` | Rhai host and core script API |
| `capabilities` | Mechanisms a mode combines, a module each: `combat`, `navigation`, `orders` and the rest ([Capabilities](04-capabilities/00-overview.md)) |
| `package` | Reads a mode's packages and every package it depends on, and runs the load checks of [Script API](08-script-api.md) |
| `runner` | Builds a match from checked packages: wires `sim`, the declared capabilities and `script`, feeds inputs |
| `verifier` | CLI: replays a session log segment, checks the result |
| `det-ci` | Headless matches of the reference MOBA with its bots on every OS, comparing state hashes |
| `lan-check` | On request: the real server and two `client --bot` processes over WebTransport on `127.0.0.1`, checked from their JSON logs and by the verifier |
| `server` | Headless app: host config, lifecycle, saves, validation, admin |
| `net` | Lightyear over QUIC (WebTransport): handshake, replication; internal |
| `launcher` | Small app: fetches, checks and starts the engine release a server or replay names; server browser |
| `client` | Bevy app: rendering, input, UI, audio, prediction |
| `content` | What names package data from outside a package: paths in a package, fingerprints; later signatures, pinning, cache, Blossom fetch |
| `editor` | Map and content editors |
| `identity` | Nostr keys, session keys, listings, reputation |
| `ownership` | License checks (optional) |
| `payments` | Pools, hold invoices, wallet budgets (optional, separate repo) |

`sim` is pure: state and inputs in, next state out; no files, packages or signatures.

`det-ci` and `lan-check` are checks, not engine crates: they live in `source/checks/`, apart from `source/crates/`, and nothing depends on them.

Dependencies: `server`, `client`, `verifier`, `det-ci` → `runner` → `package` → `capabilities` → `script`, `sim`, `content`; `script` and `sim` → `math`; `protocol` → `math`. The runner joins `protocol` and the packages: the session log and the packages each own their fingerprint type, and the runner converts between them. `math` holds what both sides share: the segment seed and the player slot. Within `capabilities`, a module imports only from the capabilities below it.

Outside the engine crates: the reference MOBA and bots. `det-ci` uses both as test content; nothing else in the engine depends on them. Bots produce inputs like players, so replays never depend on bot code.

## Capabilities

The core has no genre code; a mode combines capabilities, one native mechanism each: [Capabilities](04-capabilities/00-overview.md).

## Lifecycle and sessions

- Engine states: **waiting** (players connect, packages load, start gates pass) → **running** → **ended** (result or aborted; the log is sealed, end hooks run).
- Start gates and end hooks are how optional modules join the lifecycle; the engine has no money code. The `payments` module adds a start gate that locks stakes and an end hook that settles them.
- Everything inside running (pick, rounds, buy time, overtime) is defined by the mode script.
- `ctx.end` is optional: a persistent world never calls it.
- A session outlives the server process. A match restores by replaying its own log from tick 0; a world loads its latest checkpoint and replays the log after it. Players reconnect with the same session key.
- The server writes each input to the log before its tick runs and flushes the log from a background thread, so no tick waits on the disk; a crash loses at most the unflushed inputs, and clients resync to the restored state.
- A match that is not back within the host's restore window aborts. The window must end before any stake's hold invoice expires; a window of 0 means a crash always aborts.
- Script state is versioned; migration hooks convert saved state when a package version changes at restart.

## Inputs

Anything from outside enters the sim as a recorded input: player inputs (commands, each in its capability's format), bot inputs, player connects and disconnects, character loads, admin commands, payment events, calendar time. If it is not in the log, the sim cannot depend on it.

Clients can join a running game at any time; they receive the current state of what they can see.

## Session log

- A match is one segment; a persistent world checkpoints every few minutes. Format: Protocol Spec.
- The verifier replays any segment from its checkpoint. Hosts set how long logs are kept.
- **Snapshot:** the postcard encoding of every sim component and resource, entities sorted by stable id, component types in an order the engine release fixes, script state maps sorted by key. Its format belongs to the engine release, not the protocol.
- **State hash:** one BLAKE3 hash for each state type over the same bytes the snapshot holds, then one hash over the list of `(name, type hash)` ([Determinism Core](09-determinism-core.md)). `det-ci` compares it on every tick and, at the first mismatch, the per-type hashes, to name the first divergence.
- **No slow tick for a checkpoint:** at the tick boundary the server copies only the components changed since the last checkpoint; a background thread applies them to its copy, encodes and hashes it, and writes the checkpoint record when done.

## Scripting

`script` is its own crate in the deterministic core, so `sim` is testable without Rhai and modders get a stable API. Scripts run inside the sim tick.

- **Narrow game API.** Scripts never touch the ECS; they call the script API.
- **No `bevy_mod_scripting`.** It exposes all Bevy types and [pins Bevy patch versions](https://lib.rs/crates/bevy_mod_scripting_script).
- **Rhai engine:** `Engine::new_raw` with only the packages scripts need; no `eval`, imports, floats or time; `print` goes to debug logs only; a fixed hashing seed; never `unchecked`.
- **Operation limits:** a limit per call, and per-tick pools: one per player slot for the calls that player causes, `think` for AI, `mode` for mode hooks. The manifest sets them; counts are the same everywhere, so an over-budget script fails the same everywhere. An AI whose pool is spent stays due and thinks first next tick.
- **Other limits** (call depth, sizes) are engine constants, set explicitly, since Rhai's defaults differ between debug and release builds.
- **All or nothing per call.** State writes go to an overlay the call can read back; engine effects (damage, spawn, orders, timers) are queued. On success the overlay commits, then the effects apply in call order. On failure (error, overflow, limit) both are discarded, the sim emits a `script_error` event, and the tick goes on.
- **No hidden script state.** It is declared in a typed schema and stored in sim components; see [Script state](03-game-scripting.md#script-state).

## Backends

Collision, pathfinding and visibility each have one interface and pluggable backends, chosen per mode: [Navigation](04-capabilities/navigation.md), [Vision](04-capabilities/vision.md). Collision: circles on a plane (now), static 3D level geometry, or `physics`. A physics backend may use strictly deterministic floating point inside itself, such as Rapier's [`enhanced-determinism`](https://rapier.rs/docs/user_guides/rust/determinism/) mode, pinned per release and checked by `det-ci`; everything else stays fixed-point, and a NaN panics before it reaches a snapshot.

## Bevy

- `sim` depends on `bevy_ecs` only and is one schedule. The server and client run it inside Lightyear's fixed tick; the verifier and `det-ci` run it in a bare `World`. The server never links the renderer. Pinned to [Bevy 0.19](https://bevy.org/news/bevy-0-19/); the script API and protocol expose no Bevy types.
- Sim systems touch only sim components, so Lightyear's components cannot change a result.
- The server records the inputs received since the last tick, then runs the tick. It hashes the state at checkpoints and at the result; a hash after every tick is opt-in.
- Each unit replicates to the clients whose team sees it. A client predicts only what its player controls (position, destination, death and respawn), with no input delay: Lightyear keeps its tick ahead by the round trip, so its inputs land in time. A rollback reruns the sim from the server's state. It predicts movement, never a random outcome.
- `client` draws each unit with its own entity, interpolated between ticks; floats (`Transform`) exist only there.
- **Measured** (i9-13980HX): a packet's signature costs 17 µs to sign and 26 µs to check; a 3v3 tick at 20 Hz costs 55 µs on average and 363 µs at worst; a re-simulated tick of the lane 1v1 costs 3.2 µs. A 3v3 server tick with 6 packets costs at most about 0.5 ms of its 50 ms, and an 8-tick rollback at a 200 ms round trip at most about 3 ms. Decision 1 holds.

**Determinism rules for `sim`** (numbers, RNG and the state hash: [Determinism Core](09-determinism-core.md)):

- One fixed-tick schedule in ordered stages. Two systems with conflicting access and no order fail the build.
- Stable entity ids, never Bevy `Entity`, for the protocol, replays and every order that matters; Bevy [does not guarantee query order](https://docs.rs/bevy_rand/latest/bevy_rand/tutorial/ch02_basic_usage/index.html).
- Positions are 3D and fixed-point, 1 unit = 1 meter, within ±2²⁰ m. No floats outside a physics backend, no randomly seeded hash maps, no wall clock. An overflow panics in every build.
- The RNG is a cryptographic PRF, so clients cannot recover the seed from outcomes. A random value that decides an outcome never reaches a client before the log is published: clients predict effects, never results.

## Testing and diagnostics

- **Match scenarios** run whole matches between scripted players (`OrderScript`) in the test suite, through a modeled link of delay, jitter and loss on a manual clock, so each run repeats. Lightyear measures round trips by the wall clock, so the harness adds the modeled round trip to the sync margin, and its frame costs under delay are not real ones.
- **LAN check** (`campfire-lan-check`, on request): the real server and two `client --bot` processes on `127.0.0.1`, checked from their JSON logs and by the verifier.
- **CI** runs the check chain and the LAN check on Linux, Windows and macOS; each platform's verifier then replays every platform's session log.
- **Logging** goes through `tracing`, never a print. `sim` and `capabilities` log nothing; they report through resources the runner logs. Binaries log to standard error, and to JSON lines with `CAMPFIRE_LOG`. An event a tool reads back is a typed `LogEvent`, with a round-trip test.

## Networking

The open protocol is separate from the transport:

- **`protocol`** (versioned, documented): session log (headers, signed inputs, checkpoints, results). Everything a replay needs.
- **`net`**: [Lightyear](https://github.com/cBournhonesque/lightyear) for transport, replication, prediction with rollback, interpolation and interest management. Lightyear replicates through [`bevy_replicon`](https://github.com/simgine/bevy_replicon), whose per-client entity visibility the visibility backend drives. Internal: client and server always run the same engine release.

## Libraries

Exact versions are pinned across the workspace. Each release tag also pins its Rust toolchain (`rust-toolchain.toml`) and `Cargo.lock`, so release builds are reproducible: anyone can rebuild a tag and compare hashes with the published binaries. Hashes cover the unsigned build outputs, because operating-system code signing changes the bytes. Release trust follows [TUF](https://theupdateframework.io/docs/security/): several offline keys, a signature threshold, and revocation.

| Area | Crate | Notes |
| --- | --- | --- |
| Fixed-point numbers, trig, sqrt | Own code in `math` | `Num`: `*` and `/` round to nearest, ties to even; exact decimal parsing; `sqrt` from an `f64` estimate that integer steps correct to the exact root, so the result does not depend on the float; `sin_cos`, `atan2` by series at high internal precision, within 1–2 ulp. `fixed` rounds `*` toward −∞ and constants down, and `fixed_analytics` reaches 48 ulp in `atan2`, so neither is used ([Determinism Core](09-determinism-core.md)) |
| RNG | `blake3` keyed hash, wrapped in `math` | A PRF by specification, counter-based as in [Random123](https://www.thesalmons.org/john/random123/papers/random123sc11.pdf) but cryptographic, unlike Philox; known-answer vectors run in `det-ci`. Range sampling is own code. No `rand`, which [may change output in minor releases](https://www.rustmax.net/library/rand-book/crate-reprod) |
| Protocol encoding | `postcard` | [Stable wire format](https://postcard.jamesmunns.com) since 1.0 |
| State hashes | `blake3` | Per tick, so speed matters |
| File and package fingerprints | `sha2` | SHA-256, as Blossom addresses blobs |
| Nostr (`identity`, `ownership`) | `nostr`, `nostr-sdk`, `nostr-connect` | Still alpha |
| Lightning (`payments`) | `nwc` | Drives the host's own wallet, including [hold invoices](https://getalby.com/blog/build-conditional-payment-logic-into-your-app). No embedded node |
