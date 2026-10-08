# Review — every crate except `campfire-capabilities`

Whoever addresses an item deletes it. Items whose fix lives in `campfire-capabilities` stay where they answer what an undeclared or unused capability costs; each says so.

Five root causes hold most items. Each group's first paragraph gives the design that removes the whole class; its items are the places that change.

## Owner-only state replicates to every observer [medium]

- [ ] source/crates/campfire-net/src/net_protocol.rs:94,102,106,107,110 — `SpawnPoint`, `Respawn`, `Route`, `Progress` and `ModifierClocks` replicate to every observer, though only the owner's prediction reads them; `Progress` changes every tick a unit walks, and `Route` resends its whole `Vec` on change. Target: an immutable `OwnedBy(Option<PlayerSlot>)` on each unit, a replicon `VisibilityFilter` scoped to these five components, with `PlayerLink` as its client component. Blocked: see `review-crates_QUESTIONS.md`, "Owner-only replication needs `bevy_replicon` as a direct dependency".

## A match build repeats the load's work [low]

- [ ] source/crates/campfire-package/src/mode_packages.rs:179-194 with source/crates/campfire-runner/src/match_build.rs:50 — each match parses every script again, though the load parsed them (mode_packages.rs:385-386) with the same engine setup. The verifier builds a match for each checkpoint (`campfire-verifier/src/replay/mod.rs:78`). Target: the load keeps each `AST` in an `Arc` on its `Script`, and each match's host shares it. Together with the shared API modules, a match build parses nothing and binds nothing. Blocked: see `review-crates_QUESTIONS.md`, "Sharing the parsed scripts needs Rhai's `sync` feature, or a cache that stays on one thread".

## Storage grows or serializes in steps that copy [low]

- [ ] source/crates/campfire-protocol/src/session_log/mod.rs:92 — six buffers hold the whole session: `inputs`, `payloads`, `packets`, `server`, `entries` and `tick_ends`. They grow by `push` in `record`, `record_server` and `seal` (:667, :859, :1054, :1622). Each doubling copies the full history within one tick. Target: one `PagedVec<T>` type of fixed-size pages, which all six use, so a push never moves earlier data.
- [ ] source/crates/campfire-protocol/src/session_log/mod.rs:1277 — `put(out, input.payload)` serializes `&[u8]` as a seq through `postcard::to_io` (:1721), with one `write_all` per byte. The same applies to `Wire::Bot.payload` (server_input/mod.rs:69) and `InputWire.payload` (checkpoint/log_carry.rs:84). Target: one `Bytes` wrapper type that serializes with `serialize_bytes`, which all three use. The wire bytes stay the same.
- [ ] source/crates/campfire-store/src/exchange/mod.rs:33 — both channels are unbounded `mpsc::channel()`, and its list flavor boxes a block every 31 messages, against the type's doc (:8). Target: `mpsc::sync_channel(1)`.
- [ ] source/crates/campfire-math/src/u256/mod.rs:51 — `U256::round_div` does a 128-step long division even when `high == 0`, on a per-hit path (capabilities `projectiles/flights.rs:192`). Target: a native `u128` path when the value fits.

## Secrets print through `Debug` [low]

Design: one `Secret<const N: usize>` type in `campfire-common` holds every secret byte array. Its `Debug` writes the redacted form, and no other type holds secret bytes.

- [ ] source/crates/campfire-common/src/segment_seed.rs:4 — `SegmentSeed` derives `Debug`. The derived `Debug` of `SimRng` (campfire-sim/src/sim_rng.rs:10), `RngSource` and `RngOpener` (campfire-math/src/rng/rng_opener.rs:10) carries it into each `{:?}` and panic message.
- [ ] source/crates/campfire-protocol/src/seed_chain/mod.rs:12 — `SeedChain` derives `Debug`, and its root shows every segment's seed. `ServerSeed` (server_seed.rs:11) and `SessionPrivate` (session_private/mod.rs:15) also derive it, and net keeps all three in `Debug` resources.

## Data in strings, and foreign errors flattened [low]

- [ ] source/crates/campfire-package/src/files/package_header.rs:15 — a package name is a raw `String` here and in files/mode_manifest.rs:27 and locale_package.rs:19-20. But an avatar's package name becomes a unit type name (mode_packages.rs:257, load_check.rs:864) and a locale path component. Target: a checked `PackageName` newtype with the rules of `DeclaredName`.
- [ ] source/crates/campfire-package/src/package_files.rs:73-74 — a missing file and non-UTF-8 text both go into `ContentError::Io` as an `io::Error`. Target: the cases `ContentError::Missing { path }` and `ContentError::NotText { path, error: Utf8Error }`.
- [ ] source/crates/campfire-package/src/language.rs:10,24 — `Language` keeps the text, and it parses a `LanguageIdentifier` on every call to `identifier()`. Target: hold the parsed identifier.
- [ ] source/crates/campfire-net/src/sim_server/bot_driver.rs:191-193 — a bot payload that is too large is logged as `InputDropped { name: format!(…) }`, which puts prose in a field that means a mode input's name. Target: an event of its own, with typed fields.
- [ ] source/crates/campfire-net/src/sim_client/mod.rs:541 and source/crates/campfire-net/src/sim_server/bot_driver.rs:153 — `OrderDropped.action` and `AvatarMissing.action` are `format!("{:?}", action)`. Target: the field is the typed `Action`.
- [ ] source/crates/campfire-log/src/logging.rs:36 — `ChosenFilter.refused: Option<String>` turns `env::VarError` and the filter's `ParseError` (:108, :116) into text. Target: an enum of the two cases that keeps each error's type.
- [ ] source/crates/campfire-protocol/src/delegation/mod.rs:95 — `Event::from_json(json).ok().ok_or(DelegationError::NotEvent)` drops the nostr error. Target: `NotEvent` carries the error as `#[source]`.

## The JSON log file writes on the thread that logs [low]

- [ ] source/crates/campfire-log/src/logging.rs:62 — the file layer writes through an unbuffered `Mutex<File>`, with one `write(2)` per event on the thread that logs, the server tick included. The design lets this file stay outside the workers (a Decide item). Target: a writer that store's `Worker` owns, fed by a bounded queue. `tracing-appender` would also do it, but it is a new dependency to propose.

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
