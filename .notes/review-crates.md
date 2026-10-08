# Review — every crate except `campfire-capabilities`

Whoever addresses an item deletes it. Items whose fix lives in `campfire-capabilities` stay where they answer what an undeclared or unused capability costs; each says so.

Five root causes hold most items. Each group's first paragraph gives the design that removes the whole class; its items are the places that change.

## Owner-only state replicates to every observer [medium]

- [ ] source/crates/campfire-net/src/net_protocol.rs:94,102,106,107,110 — `SpawnPoint`, `Respawn`, `Route`, `Progress` and `ModifierClocks` replicate to every observer, though only the owner's prediction reads them; `Progress` changes every tick a unit walks, and `Route` resends its whole `Vec` on change. Target: an immutable `OwnedBy(Option<PlayerSlot>)` on each unit, a replicon `VisibilityFilter` scoped to these five components, with `PlayerLink` as its client component. Blocked: see `review-crates_QUESTIONS.md`, "Owner-only replication needs `bevy_replicon` as a direct dependency".

## A match build repeats the load's work [low]

- [ ] source/crates/campfire-package/src/mode_packages.rs:179-194 with source/crates/campfire-runner/src/match_build.rs:50 — each match parses every script again, though the load parsed them (mode_packages.rs:385-386) with the same engine setup. The verifier builds a match for each checkpoint (`campfire-verifier/src/replay/mod.rs:78`). Target: the load keeps each `AST` in an `Arc` on its `Script`, and each match's host shares it. Together with the shared API modules, a match build parses nothing and binds nothing. Blocked: see `review-crates_QUESTIONS.md`, "Sharing the parsed scripts needs Rhai's `sync` feature, or a cache that stays on one thread".

## The JSON log file writes on the thread that logs [low]

- [ ] source/crates/campfire-log/src/logging.rs:62 — the file layer writes through an unbuffered `Mutex<File>`, with one `write(2)` per event on the thread that logs, the server tick included. The design lets this file stay outside the workers (a Decide item). Target: a writer that store's `Worker` owns, fed by a bounded queue. `tracing-appender` would also do it, but it is a new dependency to propose. Blocked: see `review-crates_QUESTIONS.md`, "The JSON log file's writer: a worker of `store`, which the design does not let `log` use".

## Layout, visibility and guide rules [low]

- [ ] source/crates/campfire-protocol/src/session_log/mod.rs:162 — `StampCount` and `Spill` are wire types that `checkpoint/log_carry.rs:7` also imports, but they sit in the 1765-line `SessionLog` file. Target: a file for each.
- [ ] source/crates/campfire-protocol/src/receipt/mod.rs:85 — `SignedReceipt::encode() -> Vec<u8>` and `SessionPrivate::encode() -> Vec<u8>` (session_private/mod.rs:23) return new buffers, but `SessionLog::encode(&self, out)` takes an out-param. Target: one shape, `encode(&self, out: &mut Vec<u8>)`.
- [ ] source/crates/campfire-store/src/append_writer/mod.rs:37 — `AppendShared` and the `AppendWatch` field (append_watch.rs:11) are `pub(crate)`, but only `append_writer` uses them. Target: private, and `pub(super)` for the field.
- [ ] source/crates/campfire-client/src/view/mod.rs — `View::float` is a general `Num` to `f32` conversion, but it is a function of the plugin. Target: the conversion on a type that owns it.
- [ ] source/crates/campfire-client/src/args/mod.rs:13 — `pub(crate) mod error;` is used only in `args`, and the same is true of `server_tls/mod.rs:12`. Target: private.
- [ ] source/crates/campfire-server/src/server_config.rs:7 — `ServerConfig(ServerSetup)` is always inserted (main.rs:123), but only `Restore::run` (opening/mod.rs:146) reads it. Target: the setup goes into `Restore`, and `ServerConfig` goes.
- [ ] source/crates/campfire-net/src/sim_server/bot_driver.rs:17 — the server imports `crate::sim_client::bot_script::BotScript`. Target: `BotScript` goes beside `order_script`.
- [ ] source/crates/campfire-package/src/mode_packages.rs:127 — `pub const fn mode()` has no caller. `stat_graph` (:287) and `slotted_ranks` (:313) are `pub`, but only the crate calls them. Target: delete `mode()`, and make the two `pub(crate)`.
- [ ] source/crates/campfire-runner/src/input_rules.rs:24 — `#[cfg(feature = "internals")] pub const ROOMY` is gated in the middle of an `impl`. Target: the gated `internals` module at the end of the file.
- [ ] source/crates/campfire-script/src/script_host/mod.rs:45 — `ScriptId::nth` can be a `const fn`. The same is true of `ground_offset` and `within_ground` in `campfire-sim/src/position.rs:38,45`, through `Vec3::checked_sub`.
- [ ] source/crates/campfire-common/src/lib.rs:6 — the crate doc says the crate depends only on `serde`, but `Cargo.toml` also has `derive_more`. Target: make the two agree.

## Design and code disagree [low]

- [ ] source/crates/campfire-runner/src/session/error.rs:24,34 — `02-engine-core.md:103` says `StartError` has no data case. But it has `MatchStart(CallError)`, an `on_match_start` failure that the load cannot refuse, and `Packages(StoreError)`. Correct one of the two, and state which.
