# Workspace audit B: the network stack's crates

Scope: the 22 `source/crates/network/net-*` crates (`campfire-net-*`, copied at `HEAD` = `68ca71cbc`), and the old `net` and `quic`. Read-only; evidence from `cargo metadata --no-deps --offline`, `cargo tree --offline`, grep, and reading the code. Nothing was built or run.

Method notes. "Production" means `src/` minus `tests.rs`, `tests/`, `internals/`, `bench.rs` and the gated mods at a file's end. Counts marked *heuristic* come from line scripts and may be off by a few. The baseline "Campfire" is `crates/{base,sim,session,capabilities,apps,test}`, 1,464 files and 192k lines, against the stack's 624 files and 83k lines.

---

## 1. The crates

Direct deps are normal dependencies, optional ones included (workspace crates + external crates). Closure is the count of distinct packages in `cargo tree -e normal`.

| Crate | Kind | Responsibility | Direct (ws+ext) | Closure | Not needed or could narrow |
| --- | --- | --- | --- | --- | --- |
| net-common | problem | Grab-bag: `Arena`, `ReadyBuffer`, `SplitMix64`, OS/seeded `Random`, bench `Scale`/`Kind`, mutation fuzzing | 0+4 | 2 | Each item has 1–4 users; `SplitMix64` goes in batch "The stack on Campfire's base crates"; see B3 |
| net-core | problem | Tick/id vocabulary **and** protocol-registry hashing (`RegistryHasher`, `RegistrySeal`, `TypeMapper`, `StableHasher`) | 1+6 | 56 | `blake3` only for `StableHasher`; `fixed` (batch "The stack on Campfire's base crates"); published `HashMap` alias with no outside user; see B4 |
| net-bytes | problem | Reader/Writer/varints/`ToBytes` for the stack's headers, **plus** a serde bridge to `codec::Binary` | 2+6 | 38 | `campfire-codec` and `serde` only for `Writer::encode` / `Reader::decode`, whose one caller is net-messages; see B1 |
| net-payload | problem | The queues between the frame and each link's packet session, and the link-end codes | 0+3 | 9 | — |
| net-emulator | problem | netem `LinkModel`, `LinkConditioner`, `EmulatedNetwork` | 2+3 | 57 | Closure is mostly codec's, through net-core → net-bytes (B1) |
| net-packets | problem | The packet session, the endpoint core, **and** the emulated-wire IO `EmulatedWires` | 4+1 opt+3 | 59 | Holds an IO, so it requires net-emulator; see B12 |
| net-channels | problem | Channels, fragments, priority, compression, packing to budget | 4+6 | 63 | — |
| net-udp | problem / IO | Sealed UDP: TLS 1.3 handshake, Retry, admission, CIDs, resets, socket | 6+10 | 95 | `EmulatedServer`/`emulated_client` are public but used only by net-stepper and the crate's tests (B12) |
| net-port-map | problem | PCP / NAT-PMP / UPnP-IGD port forwarding | 3+6 | 70 | — (missing from the problem-crate test list, B14) |
| net-despawn | proxy (glue) | `DespawnReplicated`, the one allowed despawn | 0+2 | 91 | — (52 production lines; justified as a proxy) |
| net-timeline | glue | The app's network clock and timelines | 1+6 | 101 | — |
| net-link | glue | Link markers and the server fan-out relationship | 2+4 | 103 | `net-timeline` only so `LinkPlugin` can add `ClockPlugin` (B5) |
| net-connection | glue | Connection and server state, topology, host client | 4+10 | 113 | — |
| net-endpoint | glue | An IO's endpoints as components; frame↔session steps | 5+2 | 113 | — |
| net-transport | glue | `Transport` component, channel registry, receive/send systems, link iteration | 6+2 opt+8 | 115 | `bevy_utils` is required but named only in a doc comment and in `internals` (B2) |
| net-messages | glue | Message/event registration, senders/receivers, entity maps | 9+2 opt+11 | 137 | — (`bevy_replicon` for entity mapping is a decision) |
| net-sync | glue | RTT estimate, remote timeline, local timeline controller | 5+1 opt+7 | 116 | `bevy_derive` for one `Deref` (minor) |
| net-replication | glue | Replicon bridge, registry, visibility/rooms, hierarchy, control, pre-spawn, receiver history, checkpoints, Transform diffs | 9+2 opt+14 | 139 | `bevy_transform` and `bevy_math` only for `TransformDiff`/`TransformDiff2d`, which have no caller (B6) |
| net-interpolation | glue | Rule registry/resolution, delayed interpolation, frame interpolation, interpolation timeline | 7+1 opt+14 | 146 | `net-messages` only for one `RemoteEvent<SenderHello>` observer; `transform_lerp` has no caller (B6, B9) |
| net-prediction | glue | Prediction history, rollback, correction, timeline switch | 7+1 opt+11 | 147 | `net-interpolation` for rules and frame interpolation (B9); `bevy_diagnostic` for one optional plugin |
| net-stepper | test support | Discrete-event stepper over `EmulatedNetwork`; the `Stack` trait | 8+5 | 136 | — |
| net-stack | facade (glue) | Plugin groups, `ProtocolCheckPlugin`, full-stack scenarios, swarm and allocation audit | 11+4 opt+5 | 148 | — |
| **net** (old) | glue (game networking) + session host | Lightyear over QUIC; lobby, door, journal, checkpoints, saves, client join | 21+11 | 314 | `lightyear`, `lightyear_serde`, `campfire-quic` go in batch "Campfire on the stack"; `campfire-vision` (2 uses) and `campfire-players` deserve a look (B13) |
| **quic** (old) | IO + port map | QUIC under Lightyear's link; also a port mapper | 2+18 | 238 | Deleted in batch "Campfire on the stack"; its port map is the drifted original of net-port-map (B25) |

No dependency is entirely unused. The only required dependency with no production use is `bevy_utils` in net-transport (B2).

---

## 2. The layer graph as it is now

Layer = 1 + the highest layer among required workspace dependencies. Optional `net-stepper` edges (behind `internals`) are left out.

```
L0  net-common   net-despawn   net-payload
L1  net-bytes ─────────────→ codec, net-common
L2  net-core ──────────────→ net-bytes                      [+ blake3, fixed, bevy_platform]
    net-port-map ──────────→ common, store, net-common
    quic (old) ────────────→ common, store                   [+ lightyear, quinn-proto, igd-next …]
L3  net-channels ──────────→ net-bytes, net-common, net-core, net-payload
    net-emulator ──────────→ net-common, net-core
    net-timeline ──────────→ net-core
L4  net-link ──────────────→ net-payload, net-timeline        ✗ (B5)
    net-packets ───────────→ net-common, net-core, net-emulator, net-payload    ? (B12: holds an IO)
L5  net-connection ────────→ common, net-core, net-link, net-payload
    net-endpoint ──────────→ store, net-link, net-packets, net-payload, net-timeline
    net-udp ───────────────→ common, net-common, net-core, net-emulator, net-packets, net-payload
L6  net-stepper ───────────→ connection, core, emulator, endpoint, link, packets, timeline, udp
    net-transport ─────────→ channels, connection, core, link, payload, timeline
L7  net-messages ──────────→ common, bytes, channels, connection, core, link, payload, timeline, transport
    net-sync ──────────────→ connection, core, link, timeline, transport
L8  net-replication ───────→ codec, bytes, channels, connection, core, despawn, messages, timeline, transport
L9  net-interpolation ─────→ connection, core, despawn, messages, replication, sync, timeline
L10 net-prediction ────────→ connection, core, despawn, interpolation, replication, sync, timeline
L11 net-stack ─────────────→ (every glue crate) + opt endpoint, packets, udp, stepper
L17 net (old) ─────────────→ 21 Campfire crates incl. sim, runner, capabilities, vision, players, quic
```

No Campfire crate depends on a `net-*` crate yet. Outside users of the old crates: client, server, harness and lan-check (→ net, quic).

Edges that look wrong:

| Edge | Why | Finding |
| --- | --- | --- |
| net-bytes → campfire-codec (→ toml, serde_json, postcard) | Puts the 30-crate codec closure under every problem crate, for a serde bridge that one glue crate calls | B1 |
| net-link → net-timeline | Exists only for `app.add_plugins(ClockPlugin)` inside `LinkPlugin` | B5 |
| net-core → blake3 (+ registry types) | Registry hashing is a glue-layer job; ticks and ids do not need it | B4 |
| net-packets → net-emulator, and net-endpoint (glue) → net-packets | The session crate holds an IO (`EmulatedWires`), so "a glue crate never depends on an IO crate" holds only in name | B12 |
| net-prediction → net-interpolation | Prediction needs the lerp-rule registry and frame interpolation, not delayed interpolation of remote entities | B9 |
| net-interpolation → net-messages | One observer of `RemoteEvent<SenderHello>` (`interpolation_timeline_plugin.rs:13,63`) | B9 (minor) |
| net-replication → bevy_transform, bevy_math | Only for `TransformDiff`/`TransformDiff2d`, which have no caller | B6 |
| net → campfire-vision, campfire-players | Game networking that names a capability crate | B13 |

---

## 3. Convention divergences

Per-kLOC rates compare the stack (83.4k lines) with Campfire (191.8k lines in the same scan; 172–230k in some sub-scans as noted). *Heuristic* counts.

| Rule | Stack | Campfire | Examples |
| --- | --- | --- | --- |
| `pub mod` in `lib.rs` other than `internals`/`bench` | **7** | 0 | `net-bytes/src/lib.rs:18` `varint`; `net-channels/src/lib.rs:32,39` `packet`, `receive_budget`; `net-connection/src/lib.rs:22` `identity`; `net-messages/src/lib.rs:33` `entity_bytes`; `net-replication/src/lib.rs:131` `history_write`; `net-timeline/src/lib.rs:17` `rollback` |
| Nested `pub mod` under `internals`/`bench` (module paths as API) | **30** | 0 (inline `pub mod internals { pub use … }` in codec, common, math, store, package-disk) | `net-packets/src/internals/mod.rs` (`pub mod plain_host; pub mod session_counts;`); `net-common/src/bench/mod.rs` (`pub mod kind; pub mod scale;`); `net-udp/src/lib.rs:52-62` inline `pub mod bench` |
| Methods over `pub`/`pub(crate)` free fns | **141** (1.7/k) | 63 (0.33/k) | `net-prediction/src/prediction_history/mod.rs:176-410` (8 generic systems); `net-interpolation/src/interpolate/mod.rs:38-357`; `net-replication/src/replication_target.rs:134-246` |
| Docs that restate a name or lead with a verb ("Returns…", "Plugin that…", "Marker component…", "\[`X`\] is…") | **81** | 2 | `net-messages/src/message_kind.rs:6` "is an internal wrapper around the type of the message"; `net-connection/src/identity.rs` "Returns true for…"; `net-bytes/src/lib.rs:3-10` |
| Code examples in docs, never compiled (no docs lane) | **≈39** in 27 files | 0 | `net-interpolation/src/lib.rs` (≈8, one wrong, B18); `net-replication/src/pre_spawned/mod.rs:31` (` ```rust `) |
| Citations of a design decision | **46** (0.55/k); 9 crates cite nothing | 323 in 172k (1.9/k) | No citation at all in net-bytes, net-common, net-connection, net-core, net-interpolation, net-link, net-messages, net-prediction, net-stack, net-timeline, net-transport |
| Gated `impl`/`fn` before the end-of-file mods | **8** | 4 | `net-messages/src/message_sender.rs:84` `#[cfg(test)] impl`; `net-replication/src/deferred_entity_commands/mod.rs:92` gated trait method; `net-port-map/src/port_mapper/mod.rs:127` gated match arm |
| `unreachable!` in production | **28** (0.34/k) | 11 (0.06/k) | `net-channels/src/reliable_send_state.rs` (11); `net-channels/src/channel_sender/mod.rs:289-418` (5); `net-udp/src/secure_establishment/server_side.rs:428` (a server's `connect`) |
| `is_plugin_added` guards (a plugin installing its own dependencies) | **50** (≈24 production, the rest in `internals` stacks and the stepper) | 1 | `net-link/src/link_plugin.rs:25`; `net-stack/src/shared_plugin/mod.rs`; `net-timeline/src/timeline_plugin.rs` |
| String in an error | 1 (+ test-only scenario code) | 32 | `net-port-map/src/error.rs:13` `NoGateway(String)` |
| One major type per file (files with 2+ top-level types) | 106/624 = 17% | 539/1464 = 37% | On par or better |
| File named after none of its types | 19 (0.23/k) | 44 (0.23/k) | On par |
| `bool` parameter | 22 (0.26/k) | 48 (0.25/k) | On par; some do select behaviour: `net-channels/src/channel_sender/mod.rs:425` `finish_send(discard_unsent)`, `net-channels/src/single_data.rs:38` `read(…, has_id)`, `net-messages/src/endpoint_side.rs:19` `sends(remote_client)` |
| 4+ params, not a Bevy system | 66 (0.8/k) | 328 (1.7/k) | Lower than Campfire |
| Tuple returns | 0 outside the bundle-lerp API, which is a decision (api.md) | 3 | — |
| Struct with no `Debug` | 0 real (the lint holds it) | — | — |
| Fat inline test mod (>150 lines or >40%) | 23 (0.28/k) | 60 (0.31/k) | On par: `net-messages/src/multi_message_sender.rs` (266/473), `net-replication/src/server_replicon_plugin.rs` (199/503) |
| `use super::` in production / `#[allow(` | 0 / 0 | 2 / 3 | Better |
| Pub fields | 3/k | 8/k | Better |

In short, the stack is as tight as Campfire, or tighter, on file layout, fields, parameters and lints. It diverges on its published surface (module paths), on free-function systems, on Lightyear-style documentation, on missing citations, and on plugin self-installation.

---

## 4. Findings

### Dependencies

**B1. The byte codec pulls `codec` (with toml, serde_json, postcard) under every problem crate, for one glue caller.**
Evidence: `net-bytes` requires `campfire-codec` and `serde` only for `impl Sink for Writer` plus `Writer::encode` (`net-bytes/src/writer/mod.rs:44-54`), `Reader::decode` (`net-bytes/src/reader.rs:52-60`) and `SerializationError::Binary` (`serialization_error.rs:15`). The only production caller outside net-bytes is `net-messages/src/serialize_fns.rs:175`; net-replication calls `Binary` directly (`component_codec.rs:27-47`). Codec's own closure is 30 packages; with it, net-bytes reaches 38, net-core 56, net-emulator 57, net-packets 59 and net-channels 63.
Why it matters: the goal is minimal dependencies per problem. A link model, the packet session and UDP compile a TOML and JSON stack for nothing. And net-bytes holds two jobs, the stack's header codec and a serde bridge, which layering.md's own reason for refusing an `ecs` feature in net-bytes argues against.
Direction: move the serde bridge into net-messages, where its caller is, for example by encoding through `Binary::encode_into` as replication already does. net-bytes then drops `codec` and `serde`.
Confidence: high on the facts. Medium on the exact move: the orphan rule pins `impl Sink for Writer` to codec or net-bytes, so the bridge may need a newtype or to go through `Vec`.

**B2. A required dependency that only a doc comment names passes the rules test.**
Evidence: `net-transport/Cargo.toml` requires `bevy_utils` (with `debug`). The crate names it only at `src/channel_registry.rs:23` (a `///` example) and `src/internals/test_transport_plugin.rs:5`. The rule `a_required_dependency_is_named_by_production_code` reads `SourceFile::production` (`crates/base/store/src/source_files.rs:52`), which keeps comments.
Why it matters: the rule meant to keep required dependencies honest has a hole, and one stack crate already falls through it.
Direction: make `bevy_utils` optional under `internals` in net-transport, and blank comments in `production()`. The second change is a `store`/`rules` item for the issue log.
Confidence: high.

**B3. net-common is four unrelated helpers whose users are narrow.**
Evidence: `Arena` is used by net-bytes, net-channels, net-packets and net-udp. `ReadyBuffer` is used by net-emulator alone (`emulated_direction.rs`, `link_conditioner.rs`). `SplitMix64` moves to `math` in batch "The stack on Campfire's base crates". `Random` (behind `os_random`, with `ring`) is used by net-port-map and net-udp. The bench `Scale`/`Kind`/`without_logs` are used by the benches of 7 crates; `internals::mutations` by the tests of channels, packets and udp.
Why it matters: layering.md refuses "a crate two jobs". 12-lightyear-in-tree.md keeps net-common because "`net-bytes` depends on it and `net-core` on `net-bytes`". That reason is `Arena` alone, and every `Arena` user already reaches net-bytes (net-packets and net-udp through net-core).
Direction: after batch "The stack on Campfire's base crates", put `Arena` in net-bytes and `ReadyBuffer` in net-emulator. Give `Random` one home with `Entropy` (B8). Give the bench policy one home with Campfire's benches (B24). Then net-common can go.
Confidence: medium.

**B4. net-core holds the registry-hashing job, and publishes a map alias no one uses.**
Evidence: `RegistryHasher`, `RegistryHash` (`pub type … = u64`, `registry_hasher.rs:18`), `RegistrySeal`, `TypeMapper` and `TypeKind` are used only by net-channels, net-transport, net-messages and net-replication. `StableHasher` (the reason `blake3` is required) is used only by `net-replication/src/pre_spawned/mod.rs:160`. `pub use crate::collections::HashMap` (`lib.rs:22`, `collections.rs:6`) has no user outside net-core; other crates import `bevy_platform::collections::HashMap` directly.
Why it matters: every problem crate compiles BLAKE3 and registry code it never calls. `RegistryHash` is an alias, not a type. The `HashMap` alias is an un-needed published item.
Direction: move the registry pieces to the lowest glue that registers (net-channels or net-transport), use Campfire's `DomainHasher` for the pre-spawn hash (B8), and make the alias private.
Confidence: medium.

**B5. `net-link` depends on `net-timeline` only to install the clock.**
Evidence: `net-link/src/link_plugin.rs:6` and `:25-27` (`if !app.is_plugin_added::<ClockPlugin>() { app.add_plugins(ClockPlugin) }`). Nothing else in net-link names the timeline.
Why it matters: link state is transport-neutral and clock-free. The edge raises net-link a layer and ties plugin wiring to a state crate.
Direction: the facade (or each app's plugin group) adds `ClockPlugin`; drop the edge.
Confidence: high.

**B6. Features with no caller bring the Transform stack into replication and interpolation.**
Evidence: `TransformDiff` and `TransformDiff2d` (`net-replication/src/transform_diff.rs`, `transform_diff_2d.rs`) and `transform_lerp` (`net-interpolation/src/lerps.rs:16`) are exported and used only by their own tests. They are why net-replication requires `bevy_transform` and `bevy_math`. Interpolation also orders `.before(TransformSystems::Propagate)` (`frame_interpolation_plugin/mod.rs:117`).
Why it matters: api.md says "A feature exists only with a caller and a test". Campfire's sim never replicates `Transform` (02-engine-core.md, Floats only in the client).
Direction: delete them, or keep them only once a client caller exists.
Confidence: high on the facts. Medium on deleting, as batch "Campfire on the stack" may want a client-side use.

**B7. Two versions of four crates in one lock, and about 90 inline version pins.**
Evidence: the lock holds bevy 0.19.1/0.20.0, bevy_replicon 0.44.2 (git patch) and 0.45.2, quinn-udp 0.5.16 and 0.6.3 (`net-udp` pins `"=0.6.3"` inline), and xmltree 0.10.3 and 0.12.0 (`net-port-map` pins `"=0.12.0"` inline). 19 `net-*` manifests pin `bevy_*` inline rather than with `workspace = true`.
Why it matters: everything else in Campfire takes its version from the workspace. PLAN batch "Campfire on the stack" names Bevy, Replicon and quinn-udp, but not xmltree.
Direction: in batch "Campfire on the stack", move every pin to `[workspace.dependencies]` and set xmltree to 0.12.
Confidence: high.

**B8. Three OS-randomness helpers and two BLAKE3 hashers.**
Evidence:
- `campfire_protocol::Entropy` (`session/protocol/src/entropy.rs:6`), `campfire_net_common::Random` (`net-common/src/random.rs`) and `net-udp/src/os_random.rs:7` (`secret()` over `ring::SystemRandom`).
- `campfire_common::DomainHasher` (domain-tagged, `base/common/src/domain_hasher.rs`) and `campfire_net_core::StableHasher` (no domain tag, first 8 bytes).

12-lightyear-in-tree.md "One home for each shared helper" merges `Worker`, `SharedPort`, `ErrorReport` and `SplitMix64`, but not these.
Why it matters: one helper, many copies, which drift.
Direction: add them to batch "The stack on Campfire's base crates"'s merges.
Confidence: high on the facts. Medium on the merge, as `Entropy` is `internals`-gated for its fixed form.

### Boundaries

**B9. net-interpolation holds four jobs, and prediction depends on it for two of them.**
Evidence: net-interpolation (4.6k production lines, 56 files) holds:
- the rule registry and its resolution (`interpolation_registry`, `rule_building`, `rule_resolution`, `interpolation_bundle`, `into_interpolation_fn`);
- delayed interpolation of remote entities (`history_receive`, `history_seed`, `interpolate`);
- frame interpolation (`frame_interpolation_*`);
- the interpolation timeline and its sync (`interpolation_timeline*`, `synced_interpolation_timeline.rs`), which pulls in net-sync and net-messages.

net-prediction imports from it in 22 places: `HistoryComponent` ×9, `InterpolationRegistry` ×5, `InterpolationRule`/`Fns` ×6, `FrameInterpolationPlugin` ×3, `FrameInterpolate` ×3, `LerpFn`, `linear_lerp`, `FrameSamples`/`FrameSource`. That is rules and frame smoothing, not delayed interpolation.
Why it matters: prediction compiles, and orders against, the remote-interpolation pipeline it does not use. These are not clean boundaries.
Direction: a crate of lerp rules and frame interpolation below both, with delayed interpolation and its timeline above it.
Confidence: medium-low; it needs a closer read of how the registry ties history ownership to delayed interpolation (net-interpolation's crate docs, "History ownership and apply ownership").

**B10. net-replication is large, but its jobs are coupled through Replicon.**
Evidence: about 6.5k production lines plus 6k of tests. Sizes by job (production lines):

| Job | Lines |
| --- | --- |
| Replicon bridge | 1,214 |
| Targets and control | 1,052 |
| Registry and codec | 980 |
| Receiver history (`ConfirmedHistory`, `HistoryDiffReceiver`, `history_write`) | 907 |
| Hierarchy | 809 |
| Visibility and rooms | 781 |
| Pre-spawn | 605 |
| Checkpoints and hello | 581 |
| Despawn and lifetime | 411 |
| Transform diffs | 122 |

Interpolation and prediction take `ConfirmedHistory` ×19, `ReplicationCheckpointMap` ×6, `HistoryState` ×7 and `HistoryDiffReceiver` ×5.
Why it matters: receiver history is the one seam a split could cut cleanly.
Direction: no change before batch "Campfire on the stack". Revisit a receiver-history crate if B9 happens.
Confidence: low.

**B11. Small crates: keep every one except net-common.**
Evidence (production lines): net-despawn 52, net-link 367, net-endpoint 413, net-payload 438, net-transport 454, net-timeline 502.
- net-despawn is a proxy (its `clippy.toml` exemption needs a crate root).
- net-payload is the session/transport seam by decision (session.md, The seam is a set of queues on `Link`). Every one of its users already reaches net-core, so a merge would cost no edge, but it would give net-core a third job.
- net-transport, net-endpoint and net-timeline are glue halves of the problem/glue split.
- net-link is keep-worthy once B5 removes its only odd edge.
Confidence: medium.

**B12. The IO boundary is fuzzy: the session crate holds an IO, and UDP publishes test-only sockets.**
Evidence: `EmulatedWires` (`net-packets/src/emulated_wires.rs`) is an IO in the session crate, so net-packets requires net-emulator, and glue net-endpoint depends on a crate that holds an IO. `EmulatedServer` and `emulated_client` (`net-udp/src/sockets/mod.rs:131,169`, exported at `lib.rs:46`) are used only by net-stepper (`stepper/mod.rs:140,239`) and `sockets/tests.rs`. net-udp's production use of net-emulator is `LinkConditioning` (`udp_config.rs:1`), which is a decision (testing.md).
Why it matters: layering.md's "A glue crate never depends on an IO crate" holds only because net-packets is not called an IO crate.
Direction: gate the emulated UDP sockets behind `internals` (net-stepper turns it on), and state in layering.md that the emulated-wire IO is the session crate's.
Confidence: medium.

**B13. `net` mixes session hosting with network glue, and names capability crates.**
Evidence: of `net`'s 129 files, 24 name `lightyear` or `campfire_quic`; `sim_server` has 40 files of which 12 do. `net` requires 21 Campfire crates, among them `campfire-vision` (`ByVision` at `sim_server/bot_driver.rs:10`, `ByHolding` at `sim_client/mod.rs:32`) and `campfire-players`. 02-engine-core.md (Contracts through `units`) says the server's sending reads vision's `SeenBy` "with no capability crate".
Why it matters: the user wants problem and glue crates. `net` is both a session host (lobby, journal, checkpoints, saves, bots) and the glue to the stack. "`net` is one crate" (02) decides only the client/server role split.
Direction: after batch "Campfire on the stack", weigh a split of the session host from the network glue, and check the vision edge against the contracts rule.
Confidence: low-medium.

### Layering

**B14. Most of the stack's layering rules have no test.**
Evidence: `rules` holds `a_problem_crate_builds_with_no_engine_under_it` with an 8-crate list (`crates/test/rules/src/manifests.rs:113-122`). That list leaves out net-port-map, which is Bevy-free. No test checks layering.md's "A glue crate never depends on an IO crate" or its dev-dependency rule (Edges). 12-lightyear-in-tree.md's Checks promise "the layer test places each stack crate"; no such per-crate placement exists beyond the problem list. 02's "Layers" holds by construction only.
Why it matters: AGENTS.md says "a rule no type can hold is held by … a test where one can see the breach".
Direction: derive the kind of each crate from one table (or a `[package.metadata]` key), and test each layering.md rule against it.
Confidence: high.

### First-class citizenship

**B15. The stack publishes module paths, which Campfire never does.**
Evidence: 7 `pub mod` in `lib.rs` (section 3) and 30 nested `pub mod` under `internals`/`bench`, giving paths such as `campfire_net_interpolation::internals::protocol::CompFull` and `campfire_net_common::bench::kind::Kind`. net-udp has an inline `pub mod bench` (`lib.rs:52-62`). Campfire's crates publish `internals` as one inline `pub mod internals { pub use … }`.
Why it matters: api.md itself decides "One path for each item: private file modules, one root `pub use`". CLAUDE.md says the same.
Direction: flatten to `pub use` at the root of `lib.rs`, `internals` and `bench`.
Confidence: high.

**B16. Systems are free functions, where Campfire makes them associated functions.**
Evidence: 141 `pub`/`pub(crate)` free fns (1.7/kLOC) against Campfire's 63 (0.33/kLOC). Most are generic systems in prediction, interpolation and replication: `net-prediction/src/prediction_history/mod.rs:176-410`, `prediction_registration.rs:24-112`, `rollback_plugin/mod.rs:529-826`, `net-interpolation/src/interpolate/mod.rs:38-357`. Campfire's systems are associated fns (`SimServer::at_boundary`, `Lobby::take_joins`), as are parts of the stack (`LinkPlugin::unlink`, `InterpolationTimelinePlugin::handle_connect`).
Why it matters: CLAUDE.md, "Methods over free fns".
Direction: move them onto their plugin or their component type as each file is touched.
Confidence: high (heuristic count).

**B17. Lightyear-style documentation: descriptive crate docs, unrun examples, few citations.**
Evidence:
- 81 name-restating or verb-led doc lines against Campfire's 2.
- About 39 code examples in 27 files; Campfire has none, and there is no docs lane to compile them (02, No docs lane). One is already wrong: `net-interpolation/src/lib.rs:113,115` import `ConfirmedHistory` twice.
- `net-replication/src/lib.rs:76-92` runs several reference-link definitions onto single lines (`[`Replicate`]: … [`ReplicationTarget<()>`]:`), which Markdown does not read as definitions.
- Docs on private `mod` lines, `net-interpolation/src/lib.rs:329,338,362` ("Commonly used items for client-side interpolation", a prelude remnant).
- net-interpolation's crate doc is about 250 lines of tutorial; `net-bytes/src/lib.rs:3-10` restates itself; `net-prediction/src/lib.rs:1` "Handles client-side prediction".
- Narrating comments: `net-transport/src/internals/test_transport_plugin.rs:19,25`, `net-replication/src/pre_spawned/mod.rs:135`.
- 46 citations in 83k lines (0.55/k, against 1.9/k); 11 crates cite nothing, while 13-net-stack holds 155 decisions.

Why it matters: CLAUDE.md Comments and Documents rules, and AGENTS "Code cites a decision". Unrun examples drift silently.
Direction: a comment pass per crate as it is touched: drop the examples and tutorials, keep one-line `///`, and cite 13-net-stack decisions at the code that holds them.
Confidence: high.

**B18. Gated code sits mid-file, and `internals` is a tree in 10 crates.**
Evidence: 8 gated `impl`/`fn` items before the end mods (section 3). `src/internals/` directories hold both fixture trees (stacks, protocols), which the fixture-tree exception covers, and measurement types that read privates (`net-packets/src/internals/session_counts.rs`, `net-channels/src/internals/*_counts.rs`), which CLAUDE.md puts "beside the type whose privates it reads — no aggregator".
Direction: move gated impls to the end-of-file `internals` mod; keep the `internals/` trees for fixtures only.
Confidence: medium.

**B19. A few code-shape points; most of the code is on par.**
Evidence: 28 `unreachable!` (0.34/k against 0.06/k), 11 of them in `net-channels/src/reliable_send_state.rs`, where an enum per state could exclude the arm ("What can be forgotten is a type"). A trait method one side never implements: `net-udp/src/secure_establishment/server_side.rs:428` (`connect` is `unreachable!` on the server). `net-port-map/src/error.rs:13` `NoGateway(String)`. Everything else in section 3 is on par or better.
Confidence: medium.

### How systems are installed and ordered

**B20. `Module` is for a match's modules; the stack should stay on Bevy `Plugin`s, as `net` does.**
Evidence — the two approaches:
- Campfire's `Module` (`capabilities/core/declare/src/module.rs:11-28`) is one entry of the release's list. It carries a `Capability`, `NEEDS`/`USES`, a `declare` walk read by the install, the script API, the network's state-type lists and the audit, and an `install(&mut InstallCx<Self::Set>)` that adds systems only into the module's own `ModuleSet` on the sim's schedule. Each set is placed by `SetPlace` (stage, gap, at a step, after or before) against modules it builds on (`module_set.rs`), with role filtering, books and facts. The sim schedule is strict and single-threaded through `campfire_schedule::StrictSchedule`.
- The stack is 53 `Plugin` impls, 3 plugin groups and 16 `SystemSet` enums across `First`, `PreUpdate`, `Update`, `FixedUpdate`, `PostUpdate` and `Last`, each layer configuring its own sets and about 24 production plugins adding their dependencies through `is_plugin_added`.
- Campfire's own frame code, `net`, already uses plain plugins: `SimServer` (`net/src/sim_server/mod.rs:118-260`) chains `ServerFrameSystems` in `Update` after Lightyear's sets, and `SimServerPlugins`' `OneThread` makes `Update` and `FixedUpdate` strict (`sim_server_plugins.rs:50-63`).

Verdict: `Module` carries sim-only concepts (capability, books, script API, `SimSet` stages, role), so the stack should not implement it. The seam is in `net`: `NetProtocol` registers the declared state types with the stack's registration (batch "Campfire on the stack"), which keeps "declared once" true.
What does diverge is self-installation: Campfire composes with plugin groups and explicit lists (AGENTS, Bevy's own mechanisms, plugin groups), while the stack's layers install their own dependencies, about 24 guards in production against 1 elsewhere in Campfire.
Direction: let the plugin groups (net-stack, `net`) list every layer, and drop the guards.
Confidence: high on the description, medium on the guard removal.

**B21. No stack test runs a strict schedule, though the stack's systems land in Campfire's strict `Update` and `FixedUpdate`.**
Evidence: no `StrictSchedule` or ambiguity setting in any `net-*` crate. `net-stack/src/shared_plugin/tests.rs:189` only sets the single-threaded executor. The stack adds systems to `Update` (17 `add_systems` sites) and `FixedUpdate` (17). 12-lightyear-in-tree.md, "The server's frame stays strict", says an ambiguity among them "fails the build"; today that is first found in batch "Campfire on the stack", through the harness.
Why it matters: batch "Campfire on the stack" is one commit that also ports Bevy. An ambiguity storm there is avoidable.
Direction: the stepper applies `StrictSchedule` to `Update` and `FixedUpdate` on each app (net-stepper gains `campfire-schedule`), so each layer's tests hold it now.
Confidence: high on the facts. Not checked: whether the stack has ambiguities today.

### Names

**B22. Words that Campfire's Names section (02) gives another meaning.**

| Word | Campfire meaning | Stack uses | Where |
| --- | --- | --- | --- |
| owner | the seat that owns a unit | the rule that owns a history or an apply | `net-interpolation/src/interpolation_owners.rs:25` (`Owners`, `InterpolationOwners`), `refresh_owners` |
| part | a component an entity carries | pieces of an endpoint | `net-packets/src/endpoint_parts.rs:14` `EndpointParts` |
| slot | a unit's action or item slot | per-channel storage | `net-channels/src/channel_slots.rs:7` `ChannelSlots` |
| kind | a member of a closed set | a `TypeId` wrapper (an open set) | `ChannelKind(pub TypeId)` `net-channels/src/channel_kind.rs:7`, `ComponentKind`, `MessageKind`; also `net-common` `bench::Kind` |
| `*Data` | a section as a package writes it | wire payload pieces | `MessageData`, `FragmentData` (`fragment_data.rs:13`), `SingleData` (`single_data.rs:15`) |
| `*Spec` | a resolved section | a rule member's config | `net-interpolation/src/interpolation_rule_component.rs:36` `MemberSpec` |
| `Cx` (engine context) | `InstallCx`, `BookCx` | `*Context` | `MessageReceiveContext` (exported as `ReceivedMessageHeader`), `InterpolationSampleContext`, `UpdateHistoryContext`, `FragmentContext` |
| session | a hosted match | 12 renamed only `Session` → `PacketSession` | `SessionConfig`, `SessionQueues`, `SessionLink`, `SessionCounts`, `CloseSession`, `Link::session` |
| `Order` | the senders' order (`campfire_capabilities::Order`) | the order apps step in | `net-stepper/src/stepper/mod.rs:49` |
| `Clock` | `campfire_net::Clock`, Unix seconds (`net/src/os.rs:16`) | the app's network clock | `campfire_net_timeline::Clock`; both will be imported into `net` in batch "Campfire on the stack" |
| `Instant` | `std::time::Instant` | a `Duration` since the first update | `campfire_net_core::Instant`; the stepper uses both (`stepper/mod.rs:27`) |
| stack | — | the crate `net-stack` and the trait `net-stepper::Stack` | `net-stepper/src/stack.rs` |

Planned duplicates that batches "The stack on Campfire's base crates" and "Campfire on the stack" already remove: `TickRate`, `ChannelBinding`, `SplitMix64`, `LinkModel` (harness and emulator), `AppCode` (quic and net-payload), `LinkBinding` (quic and net-endpoint), `PortMap` (quic and net-port-map).
Not conflicts: registry, frame, `NetTick`, and Bevy component hooks (AGENTS' Direction uses "hook" in Bevy's sense too).
Direction: add the unplanned rows to batch "The stack on Campfire's base crates"'s "one word, one meaning" step.
Confidence: high.

### Tests

**B23. Two ways to test integrated behaviour today; one after the planned harness move.**
Evidence:
- **harness `InProcessMatch`** (`test/harness/src/in_process_match/mod.rs`, 1,148 lines) runs Campfire's real `SimServerPlugins`/`SimClient` on Lightyear with `CrossbeamIo`. Links follow its own `LinkModel { delay, jitter, loss_per_mille, seed }` in steps (`link_model.rs`) and a `DelayLine`. It steps every app in lockstep, pins goldens and the 3v3 scripted match, and fails on any warning through `LogCheck`.
- **net-stepper** drives a layer's `Stack` (`net-stepper/src/stack.rs`: `add_client`, `add_server`, `add_protocol`, `client_link`, `server_link`, `is_ready`) over `EmulatedNetwork` with netem's model, with discrete-event time, per-app frame rates, drift and stalls. Clients are `Host`, `Raw` or `Udp` (`client_kind.rs`), with a fixed ALPN `campfire-stepper` and secret. net-stack's scenarios, swarm and canary run on it, and catch only `error!` (`net-stack/src/scenario/error_capture.rs`).
- Their homes differ too: 02 "Tests live at the top" (in harness), against testing.md "the lowest crate whose stack has every layer".

Batch "Campfire on the stack" removes `CrossbeamIo`, `DelayLine` and the harness's `LinkModel` (one link model, one in-process IO), but the harness keeps its own frame loop. 12's Planned "The harness on the stepper" waits for after the slice.
Why it matters: the user asks for one way to test integrated behaviour. The stepper's `Stack` trait is general enough for Campfire's apps, and has the time model the harness lacks.
Direction: keep the planned move. Then align the log policy (`LogCheck` everywhere, or say why the stack checks only errors), and reconcile the two test-home rules in one decision.
Confidence: high on the facts, medium on the direction.

**B24. Two bench conventions.**
Evidence: the stack has one sampling policy (`Scale`) and `Kind` enum in `net-common/src/bench` (`kind.rs`, `scale.rs`). Campfire's benches build ids as strings and set sampling inline (`session/protocol/src/server_seeds/bench.rs:17-18`, `"atomic/server_seeds"`, `sample_size(10)`).
Why it matters: the stack's form is the better one ("No data in strings"), but only the stack has it.
Direction: give `Kind`/`Scale` a Campfire-wide home, which also empties net-common (B3).
Confidence: medium.

### The old crates

**B25. What of `net` and `quic` survives batch "Campfire on the stack".**
`quic` survives only through counterparts already in the stack:

| quic | Stack counterpart |
| --- | --- |
| `port_map/*` (about 3.9k lines) | `net-port-map` — the same code, drifted: `facing_socket.rs` identical, `nat_pmp_map.rs` 2 lines apart, `upnp_gateway/mod.rs` 382 lines apart (igd-next/attohttpc against ureq) |
| `pinned_key_verifier`, `spki`, `used_tokens`, `udp_port`, `recv_batch`, `reset_key`, `derived_secret`, `admission` | the same names in `net-udp` |
| `AppCode` | `net-payload` |
| `LinkBinding` | `net-endpoint` |
| `AddressLimit` | `AdmissionConfig` |
| `LinkTimes` | `SessionConfig` |
| `tests/alloc` | `net-stack/tests/alloc` |

What is Campfire's own and moves: the server's raw key derived from its own key (`quic/src/server_key.rs`), the ALPN `campfire` (`server_key.rs`, `ALPN`), `CloseCode` (`net/src/close_code.rs`), and `TlsKey`/`ChannelBinding`, already in `campfire-common`.

`net` survives whole, with these parts rewritten: `net_protocol/wire_codec.rs` (goes), `local/*` (`CrossbeamIo` → emulator), `match_clock.rs` (Lightyear `Tick` → `NetTick`), `close_code.rs`, `events/link_lost`, and the Lightyear imports in `sim_server` (12 files) and `sim_client` (2).

Note: `campfire_net::{Os, Clock}` is a Unix-seconds clock for delegations, not a network time source, though time.md cites it as "the shape of Campfire's `Os::Clock`" (B22).
Confidence: high.

---

## Not checked

- Test durations against the 1 s bound, and whether any stack test fails today (nothing was run).
- Whether the stack's systems have schedule ambiguities (B21 says only that nothing tests it).
- Panics on peer data: the `panic!`/`unreachable!`/`expect` sites sampled are logic invariants, but not every parse path was read. testing.md's seeded mutation tests are the stack's own guard.
- rustdoc intra-doc link validity (no docs lane; the broken reference links in B17 were read by eye).
- B9 and B10 split boundaries beyond import counts.
