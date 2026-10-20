# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Lightyear in the tree

The slice of [Lightyear in the tree](docs/design/12-lightyear-in-tree.md): the player hosts a LAN match and plays a singleplayer match as before, every link on Campfire's own network stack, copied from the fork's `main` into `source/crates/network/`, over its own UDP. With it go the batches of [Workspace alignment](docs/design/14-workspace-alignment.md), which give each crate one kind and one job, hold the stack to Campfire's conventions, give each shared tool one home and each word one meaning. The batches go by impact (Workspace alignment, The batches go by impact): first what is wrong now or makes each later batch safer, then the stack reshaped and renamed before Campfire names its types, then Campfire on the stack, then Campfire's own moves and words, which no batch of the slice needs. A batch never goes before one it needs. Each batch ends with `cargo drive gate` passing; a golden that moves names its batch and its cause.

### 10. One rule in one place

`areas`, `stats`, `combat`, `geometry`. It stands after the slice, as no batch needs it, and first of Campfire's own work, as it changes what a match computes: an area and an aura disagree on neutral and dead units, and a splash misses a unit at exactly its radius. Builds: a capability asks another through a contract below both.

- [ ] source/crates/capabilities/areas/src/area_holds.rs:46 — an area's targets and an aura's select through two filters that disagree on neutral and dead units (One rule in one place). → Both select through one filter; the issue log's Areas item goes.
- [ ] source/crates/capabilities/combat/src/attacks.rs:131 — a splash reaches within `<` its radius, where the one reach rule is `≤` (One rule in one place). → The splash reaches by `geometry`'s rule; the issue log's Combat item of the splash goes.
- **Check:** a test places a unit at exactly a splash's radius and finds it hit; an area and an aura select the same units among neutral and dead ones; each golden that moves names this batch; `cargo drive gate` passes.

### 11. Campfire's problem crates

`values`, `visibility`, `state-types`, `sim` (the plain form of `StableId`), `actions`, `progression`, `client` (charges), `client-files`; the directories of `geometry`, `schedule` and `verifier`; the workspace's `Cargo.toml`; 02 (the crates table). It stands after the slice, as no batch of it needs these moves, and before Campfire's words, which then rename each file once in its final crate; it takes the ECS out of every reader of a plain value. Builds: each crate depends only on lower layers; a capability asks another through a contract below both.

- [ ] source/crates/capabilities/core/values/src/team.rs — four components and two holders of `StableId` put the ECS under every reader of a plain value (`values`, `visibility` and `client-files` are problem crates). → The derives and impls of `Team`, `UnitType`, `UnitTags` and `Relations` go behind `values`' `ecs` feature; `Hit` and `DashDelivery` hold the plain form of a unit's stable id, which `sim`'s `StableId` converts to and from; `values` is a problem crate.
- [ ] source/crates/capabilities/core/visibility/src/setting/mod.rs:5 — `visibility` takes two plain types from `state-types` and the ECS for one derive (`values`, `visibility` and `client-files` are problem crates). → `DataKind` and `Recipients` move into `visibility`, which `state-types` names; `OutView`'s `Resource` derive goes behind `ecs`; `visibility` and `client-files` are problem crates.
- [ ] source/crates/capabilities/progression/src/learning.rs:46 — a slot's charges are read through `progression`'s `Learning` (A rule lives with its owner). → `actions` gives a slot's charges, and the HUD reads them from it; the issue log's Client item of charges goes.
- [ ] source/crates/capabilities/core/geometry/ — `geometry` sits among the ECS crates, `schedule` in `base/`, and `verifier` in `session/` (A directory names a domain, and the manifest names the kind). → `geometry` and `schedule` move to `sim/`, and `verifier` to `apps/`.
- **Check:** `values`, `visibility` and `client-files` pass as problem crates, and `cargo tree -e normal` counts fewer crates under `client-files` than at `68ca71cbc`; `cargo drive gate` passes.

### 12. Campfire's words

Every crate whose plugins, system sets, state audiences, texts or tick-rate fields are renamed; `rules`; the test packages and scripts whose `sync` key becomes `audience`; 02 (Names) and `06-script-api.md`. It stands after the moves, so each file is renamed once in its final crate; the stack's plugins and sets already have the decided names, so Campfire on the stack needs none of it. Builds: one word, one meaning.

- [ ] source/crates/apps/client/src/orders.rs — 25 plugins with bare names beside the stack's `*Plugin`s, two of them one name with a module and a trait (A Bevy plugin is a `*Plugin`). → Every plugin is a `*Plugin` and every group `*Plugins`.
- [ ] source/crates/sim/sim/src/sim_update/mod.rs — system sets end in `*Set`, `*Systems` and `*Stage` (A system set is `*Systems`). → Every system set ends in `*Systems`, `ModuleSet` among them, and `*Set` names a collection alone.
- [ ] source/crates/capabilities/core/units/src/unit_state.rs — `sync` means which clients receive a state, the timelines' sync and an fsync (Script state's receivers are its audience). → `StateAudience` and its kin, and the scripts' key `audience`; `06-script-api.md` and the test packages follow.
- [ ] source/crates/capabilities/core/values/src/message_id.rs — a localized text is a message, as a sent value is (A localized text is a text). → `TextId` and `NamedText`.
- [ ] source/crates/sim/sim/src/tick_rate/ — a tick rate's field is `rate` in 10 structs, and 31 modules gated by `test` are named `internals` (A field of a tick rate is `tick_rate`). → `tick_rate`, and the modules take names of their own.
- [ ] source/crates/capabilities/capabilities/src/order/mod.rs:23 — the issue log's Decide items of `Action`, the test packages' layout and the crate names `declare` and `state-types`, with `net-despawn`, and its item of `PlayerOrders`. → Each takes its settled name or layout, and its item goes.
- [ ] source/crates/test/rules/src/ — no test holds the suffixes (`rules` holds the suffixes). → `rules` refuses a system set not named `*Systems`, a plugin not named `*Plugin`, a `*Manager`, a `*_hook` function outside the scripts, and a `*Kind` that wraps a `TypeId`.
- **Check:** the suffix test passes; `cargo drive gate` passes.

### 13. The fold

`docs`: `02-engine-core.md` (Networking, the crates table), `05-protocol-spec.md`, `08-sessions.md`, `10-replication-audiences.md`, `04-capabilities/vision.md`, `11-quic-transport.md` and `docs/research/quic.md` (deleted). It stands last, as a slice ends when the user plays it.

- [ ] docs/design/05-protocol-spec.md:51 — the connect flow says a client connects over QUIC, which a third party implements with any QUIC library (The links run on the stack's UDP). → The spec names the stack's UDP: its handshake packets, its header and its seal, with their byte layouts and their one encoding, as a third party now implements Campfire's own protocol; the Retry and the limits per address keep their place in the flow.
- [ ] docs/research/quic.md — facts of a transport no decision takes any more. → Deleted, but each fact a decision of `13-net-stack/udp.md` cites, as `quinn-udp`'s socket, which moves into the stack's UDP research.
- [ ] docs/design/12-lightyear-in-tree.md — the slice's decisions stand in documents of their own (A slice's proposal is temporary). → Once the user plays a LAN match and a singleplayer match on the stack, each lasting decision of 12 and of `14-workspace-alignment.md` folds into 02, `13-net-stack/` or the document of the system it governs, the decisions of `11-quic-transport.md` that are Campfire's own into `08-sessions.md`, "Lightyear" and "QUIC" become the stack's names in each document and in each citation of the code, and designs 11, 12 and 14 are deleted, each citation of theirs in the code naming the decision's new home.

## Steps

**T1. The toolchain back to stable.** Touches `source/rust-toolchain.toml`. When Rust 1.100 is released, on 2026-11-12, the pin moves from the dated beta to stable 1.100, as its comment says.
- **Check:** `cargo drive gate` passes on the stable toolchain.

## Foundations

