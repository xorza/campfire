# Campfire — Engine Core

## Module map

*Diagram: module map (apps, services, deterministic core with kits) — see the live doc.*

Each layer uses the layers below it.

## Modules

| Module | Does |
| --- | --- |
| `math` | Fixed-point numbers, 3D vectors, trig, seeded RNG |
| `protocol` | Session log format (see Protocol Spec) |
| `sim` | Deterministic state and systems on `bevy_ecs`; no genre code |
| `script` | Rhai host and core script API |
| `kit-*` | One genre each (see Game Kits) |
| `runner` | Loads packages, wires `sim`, kits and `script`, feeds inputs |
| `verifier` | CLI: replays a session log segment, checks the result |
| `det-ci` | Headless bot matches on every OS, comparing state hashes |
| `server` | Headless app: host config, lifecycle, saves, validation, admin |
| `net` | Lightyear transport and replication; internal |
| `client` | Bevy app: rendering, input, UI, audio, prediction, server browser |
| `content` | Packages: fingerprints, signatures, pinning, cache |
| `editor` | Map and content editors |
| `identity` | Nostr keys, session keys, listings, reputation |
| `ownership` | License checks (optional) |
| `payments` | Pools, hold invoices, wallet budgets (optional, separate repo) |

`sim` is pure: state and inputs in, next state out; no files, packages or signatures.

Dependencies: `server`, `client`, `verifier`, `det-ci` → `runner` → `kit-*` → `script` → `sim` → `math`; `protocol` → `math`.

Outside the engine: the reference MOBA and bots. Bots produce inputs like players, so replays never depend on bot code.

## Kits

The core has no genre code. A kit is a native crate with the components, systems and script API for one genre; a game package declares the kits it uses. Packages hold only scripts and data, so a new kit ships in an engine release.

## Lifecycle and sessions

- Engine states: **waiting** (players connect, packages load, stakes lock) → **running** → **ended** (result or aborted; the log is sealed, payouts settle).
- Everything inside running (pick, rounds, buy time, overtime) is defined by the mode script.
- `match.end` is optional: a persistent world never calls it.
- A session is one run of a server process. State is saved at tick boundaries; a restart resumes from the latest save.
- Script data is versioned; migration hooks convert saved state when a package version changes at restart.

## Inputs

Anything from outside enters the sim as a recorded input: player inputs (format defined by the kit), bot inputs, character loads, admin commands, payment events, calendar time. If it is not in the log, the sim cannot depend on it.

Clients can join a running game at any time; they receive the current state of what they can see.

## Session log

- A match is one segment; a persistent world checkpoints every few minutes. Format: Protocol Spec.
- The verifier replays any segment from its checkpoint. Hosts set how long logs are kept.

## Scripting

`script` is its own crate in the deterministic core, so `sim` is testable without Rhai and modders get a stable API. Scripts run inside the sim tick.

- **Narrow game API.** Scripts never touch the ECS; they call the script API.
- **No `bevy_mod_scripting`.** It exposes all Bevy types and [pins Bevy patch versions](https://lib.rs/crates/bevy_mod_scripting_script).
- **Rhai engine:** [`Engine::new_raw`](https://docs.rs/rhai/latest/rhai/struct.Engine.html) plus needed packages only; features `no_float`, `no_time`; never `unchecked`.
- **Operation limit per call.** Counts are identical everywhere, so over-budget scripts fail identically.
- **No hidden script state.** It lives in serializable components.

## Collision

`sim` defines one collision interface with pluggable backends; each game mode picks one in its package manifest. Now: simple shapes (e.g. circles). A physics backend (vehicles, rigid bodies) may use strictly deterministic floating point inside itself, such as Rapier's cross-platform mode, verified by `det-ci`; everything else stays fixed-point.

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

- `sim` uses `bevy_ecs` only, in its own `World`; the server never links the renderer.
- `client` runs full Bevy and copies sim state into the render world each frame, interpolated. Float types (`Transform`) exist only there.
- Pinned to [Bevy 0.19](https://bevy.org/news/bevy-0-19/); upgrades are deliberate. The script API and protocol expose no Bevy types.

**Determinism rules for `sim`:**

- Fixed-tick schedule; system-order ambiguities are errors.
- Own stable entity ids (never Bevy `Entity`) for the protocol and replays, and for sorting queries wherever order matters, since Bevy [does not guarantee query order](https://docs.rs/bevy_rand/latest/bevy_rand/tutorial/ch02_basic_usage/index.html).
- Positions are 3D in every genre. No floats outside a physics backend, no randomly seeded hash maps, no wall clock. RNG state lives in the world.
- Numbers are 32.32 fixed-point (`I32F32`), 1 unit = 1 meter; the only number type scripts see. Distance math uses a 128-bit helper. Overflow checks are on in every build profile; an overflow is a bug.

## Networking

The open protocol is separate from the transport:

- **`protocol`** (versioned, documented): session log (headers, signed inputs, checkpoints, results). Everything a replay needs.
- **`net`**: [Lightyear](https://github.com/cBournhonesque/lightyear) for transport, prediction, interpolation and interest management. Internal: client and server always run the same engine release.

## Libraries

Exact versions are pinned across the workspace.

| Area | Crate | Notes |
| --- | --- | --- |
| Fixed-point numbers | `fixed` | No trig by design |
| Fixed-point trig, sqrt | `fixed_analytics` | Deterministic, panic-free |
| RNG | `rand_pcg` (PCG32) | rand [may change output in minor releases](https://www.rustmax.net/library/rand-book/crate-reprod): test output vectors in `det-ci`; never sample `usize` |
| Protocol encoding | `postcard` | [Stable wire format](https://postcard.jamesmunns.com) since 1.0 |
| Content fingerprints | `blake3` | |
| Nostr (`identity`, `ownership`) | `nostr`, `nostr-sdk`, `nostr-connect` | Still alpha |
| Lightning (`payments`) | `nwc` | Drives the host's own wallet, including [hold invoices](https://getalby.com/blog/build-conditional-payment-logic-into-your-app). No embedded node |
