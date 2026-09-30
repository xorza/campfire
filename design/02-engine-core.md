# Campfire — Engine Core

## Module map

*Diagram: module map (apps, services, deterministic core with capabilities) — see the live doc.*

Each layer uses the layers below it.

## Modules

| Module | Does |
| --- | --- |
| `math` | Fixed-point numbers, 3D vectors, trig, counter-based RNG |
| `protocol` | Session log format (see Protocol Spec) |
| `sim` | Deterministic state and systems on `bevy_ecs`; no genre code |
| `script` | Rhai host and core script API |
| `capabilities` | Mechanisms a mode combines, a module each: `combat`, `navigation`, `orders` and the rest ([Capabilities](04-capabilities/00-overview.md)) |
| `package` | Reads a mode's packages and every package it depends on, and runs the load checks of [Script API](08-script-api.md) |
| `runner` | Builds a match from checked packages: wires `sim`, the declared capabilities and `script`, feeds inputs |
| `verifier` | CLI: replays a session log segment, checks the result |
| `det-ci` | Headless matches of the reference MOBA with its bots on every OS, comparing state hashes |
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

Dependencies: `server`, `client`, `verifier`, `det-ci` → `runner` → `package` → `capabilities` → `script`, `sim`, `content`; `script` and `sim` → `math`; `protocol` → `math`. The runner joins `protocol` and the packages: the session log and the packages each own their slot and fingerprint types, and the runner converts between them. Within `capabilities`, a module imports only from the capabilities below it.

Outside the engine crates: the reference MOBA and bots. `det-ci` uses both as test content; nothing else in the engine depends on them. Bots produce inputs like players, so replays never depend on bot code.

## Capabilities

The core has no genre code. A capability is native code for one mechanism: components, systems in the engine's tick stages, a data schema, commands, and script calls and hooks. A game package declares the capabilities it uses, and any combination is valid. Packages hold only scripts and data, so a new capability ships in an engine release. See [Capabilities](04-capabilities/00-overview.md).

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
- **Rhai engine:** [`Engine::new_raw`](https://docs.rs/rhai/latest/rhai/struct.Engine.html) plus needed packages only: arithmetic, logic, basic strings, arrays, maps and iteration, not the language core package, whose `sleep` would block a tick; `eval` is disabled, imports are off, and `print` and `debug` write to standard error in debug builds and nothing in release builds, so they cannot reach the sim; features `no_float`, `no_time`, without the default `ahash/runtime-rng`; never `unchecked`. `script` fixes Rhai's hashing seed (`rhai::config::hashing::set_hashing_seed`), since Rhai otherwise seeds its hasher per build.
- **Operation limits.** One limit per call, and pools for each tick: one for each player slot, of the size `player`, for the calls that player causes (their mode inputs, and the casts of the units they control), so no other player's or script's calls can make a player's cast fail; `think` for AI, and the casts of units no player controls; `mode` for the match start and timers, later deaths and `calc_damage`; later `world` (projectile and area hooks, modifiers). The manifest sets them, and each pool holds at least one whole call. A call draws only from its pool, and the sum of the pools is the most one tick's scripts run. Counts are identical everywhere, so over-budget scripts fail identically. Once a pool is spent, later calls of its class in that tick fail; an AI unit whose call finds its pool spent stays due, and thinks first in the next tick.
- **Other limits** (call depth, expression depth, string, array and map sizes) are constants of the engine release, set explicitly: Rhai's defaults differ between debug and release builds (call depth 8 against 64), so a script could run on a release server and fail in a debug verifier.
- **All or nothing per call.** State writes go to an overlay the call can read back; engine effects (damage, spawn, orders, timers) are queued. On success the overlay commits, then the effects apply in call order. On failure (error, overflow, limit) both are discarded, the sim emits a `script_error` event, and the tick goes on.
- **No hidden script state.** It is declared in a typed schema and stored in sim components; see [Script state](03-game-scripting.md#script-state).

## Collision

`sim` defines one collision interface with pluggable backends; each game mode picks one in its package manifest. Now: simple shapes (e.g. circles). A physics backend (vehicles, rigid bodies) may use strictly deterministic floating point inside itself, such as Rapier's [`enhanced-determinism`](https://rapier.rs/docs/user_guides/rust/determinism/) mode (not with `simd8`), verified by `det-ci`; everything else stays fixed-point. Rapier's promise holds only for the same Rapier and compiler versions, which each release tag pins. Rust gives [no deterministic NaN bit patterns](https://rust-lang.github.io/rfcs/3514-float-semantics.html), so a float that is NaN is a bug that panics before it reaches a snapshot.

## Pathfinding

Same pattern: one interface, pluggable backends, chosen per game mode.

| Layer | Does | Implementation |
| --- | --- | --- |
| Long path | Route from A to B through static terrain and blockers; ignores units | Backend. Now: grid with A*, integer cells, cell size set by content. Later: navmesh |
| Local steering | Follows the route, avoids units, allows body blocking | Shared by all backends; continuous space, fixed-point, fixed unit order |
| Creep lanes | Lane routes; leave the route only to chase a target | Waypoints in content |

**Navmesh-ready interface:**

- Queries use world positions (fixed-point), never cell indices.
- A path is a list of waypoints, so steering works with any backend.
- Obstacles are added and removed as shapes; the backend decides how to apply them (flip cells, patch polygons).

## Visibility

Same pattern. Backends: grid fog of war (MOBA), 3D occlusion (FPS, battle royale). The server sends each client only what the backend marks visible to it.

## Bevy

- `sim` depends on `bevy_ecs` only and is one schedule of systems. The server and client run it inside Lightyear's fixed-tick schedule, in the app `World`; the verifier and `det-ci` run the same schedule in a bare `World` with no Lightyear. The server never links the renderer.
- Sim systems read and write only sim components and resources, so Lightyear's own components on the same entities cannot change a result. A prototype proves this before other work: one predicted unit on a Lightyear server, and a bare-`World` replay of its log, with equal state hashes on every tick.
- In Lightyear's `World`, `FixedUpdate` runs the sim schedule once per tick; sim tick 0 is the Lightyear tick the match started in. The server first records every input received since the last tick, then runs the tick. Inputs travel as Lightyear messages on a reliable, ordered channel. Sim components replicate through Lightyear; the client predicts `Position` and `Destination`.
- The predicting client sets `SimTick` from Lightyear's tick before each run, and fills `TickInputs` with its own inputs stamped for that tick. A rollback runs `FixedMain` again from the server's state, so the sim runs again with the right tick and inputs. The client runs with a dummy seed: it predicts movement, never a random outcome.
- `client` runs full Bevy and derives render state from sim components each frame, interpolated. Float types (`Transform`) exist only there.
- Pinned to [Bevy 0.19](https://bevy.org/news/bevy-0-19/); upgrades are deliberate. The script API and protocol expose no Bevy types.

**Determinism rules for `sim`:**

- Fixed-tick schedule, `SimUpdate`, one run per tick. Two systems with conflicting access and no order fail the build, and clippy bans the Bevy calls that allow such a pair. The steps of the [tick pipeline](03-game-scripting.md#tick-pipeline) are ordered sets (`SimSet`); the tick's random sequences start before the first, and the tick number (`SimTick`, which is state) advances after the last.
- Own stable entity ids (never Bevy `Entity`) for the protocol and replays, and for sorting queries wherever order matters, since Bevy [does not guarantee query order](https://docs.rs/bevy_rand/latest/bevy_rand/tutorial/ch02_basic_usage/index.html).
- Positions are 3D in every genre. No floats outside a physics backend, except as an estimate that integer steps then correct exactly (`Num::sqrt`); no randomly seeded hash maps, no wall clock.
- The RNG is counter-based: every value is `BLAKE3-keyed(segment seed, stream ‖ stable entity id ‖ tick ‖ n)`, so no draw depends on the order of other draws and systems can draw in parallel. The only RNG state is the segment seed. The function must be a cryptographic PRF: clients see many outcomes, and a non-cryptographic generator could let them recover the seed and predict hidden ones.
- A random value that decides an outcome never reaches a client before the log is published. Clients may predict effects, never results.
- Non-integer numbers are 40.24 fixed-point (`I40F24`), 1 unit = 1 meter. Distance math uses a 128-bit helper. Scripts see two number types, integers and fixed-point; see [Game Scripting](03-game-scripting.md#numbers).
- An overflow is a bug and panics in every build profile: integers through `overflow-checks = true`, fixed-point through the checked arithmetic of `math::Num`. `Num` rounds `*` and `/` to nearest, ties to even ([Determinism Core](09-determinism-core.md)).
- Every coordinate stays within ±2²⁰ m, so exact squared distances fit a `u128`.

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
