# Review — every crate except `campfire-capabilities`

Whoever addresses an item deletes it. Items whose fix lives in `campfire-capabilities` stay where they answer what an undeclared or unused capability costs; each says so.

Five root causes hold most items. Each group's first paragraph gives the design that removes the whole class; its items are the places that change.

## Owner-only state replicates to every observer [medium]

- [ ] source/crates/campfire-net/src/net_protocol.rs:94,102,106,107,110 — `SpawnPoint`, `Respawn`, `Route`, `Progress` and `ModifierClocks` replicate to every observer, though only the owner's prediction reads them; `Progress` changes every tick a unit walks, and `Route` resends its whole `Vec` on change. Target: an immutable `OwnedBy(Option<PlayerSlot>)` on each unit, a replicon `VisibilityFilter` scoped to these five components, with `PlayerLink` as its client component. Blocked: see `review-crates_QUESTIONS.md`, "Owner-only replication needs `bevy_replicon` as a direct dependency".

## Who the player commands is guessed from markers, five ways [medium]

Design: one fact, two parts. A player's units are the units its `Owner` slot names; the client learns its slot from `MatchStart`. A player's avatar, in a mode with avatars, is its owned unit whose type is one of the mode's avatar types (`ModeUnits::avatars`), a typed fact both ends hold from the packages. One method on one type answers both parts for the client, the HUD and both bots. `With<Predicted>`, `With<Experience>` and `.single()` stop being identity.

- [ ] source/crates/campfire-client/src/pointer.rs:28 — `OwnAvatar` is `single()` of `(With<Owner>, With<Predicted>)`, as in `Hud::add_gauges` (hud/mod.rs:172,176); the server predicts every owned unit (`campfire-net/src/sim_server/mod.rs:652`), so with two units it errs: no gauges, and clicks and casts order nothing.
- [ ] source/crates/campfire-client/src/view.rs:478,483 — `show_end` takes any `Predicted` entity's team by `.iter().next()`; the slot's team is known from the seat. `Hud::mark_target` (hud/mod.rs:396,401) takes the first `Predicted` entity with an attack target.
- [ ] source/crates/campfire-net/src/sim_client/mod.rs:481,521 — the client bot's `(With<Owner>, With<Predicted>)` with `.single()` makes its orders wait forever, unlogged, once it owns two units.
- [ ] source/crates/campfire-net/src/sim_server/bot_driver.rs:46 — the server bot finds its avatar by `With<Experience>`, so a mode without progression drops every bot order as `AvatarMissing`.

## The journal's record bound is not tied to the session terms [medium]

- [ ] source/crates/campfire-protocol/src/journal/journal_frames.rs:48 — `seal` has a release `assert!(len <= MAX_RECORD)` (16 MiB), but neither `SessionLog::new` (session_log/mod.rs:371) nor `record` (:643) relates the terms to it; under legal large terms a client's packet passes every check, then panics the server in `journal_last_entry` (:672) after the log changed. Target: the largest record each kind can be is a function of the terms (inputs and payload per tick, the delegation's length, a checkpoint's carry), computed once in `SessionLog::new`, which refuses terms past `MAX_RECORD` with a `HeaderError` case; `seal`'s check becomes a `debug_assert!` of a contract the log keeps.

## The client links drawings and gauges to units by hand and polls them every frame [medium]

Design: a unit's drawing is a Bevy hierarchy tied to the unit by a relationship. A `DrawingOf(unit)` relationship has a `Drawing` target on the unit with `linked_spawn`, so the unit's despawn takes the whole tree. The tree's root is an anchor with translation only, which is the glide. Its children are the body mesh, which holds the lean and the box's yaw, the gauges at a local offset that faces the camera, and the target ring, which moves under the target's anchor when the target changes. Bevy's propagation then places every child, only when its anchor moves. The per-frame placement, the despawn observer, the gauge polling and the HUD-against-view order all go away. Fills and hit marks change only on `Changed<Pools>`, `Changed<ActionSlots>` and `Added<Dead>`.

- [ ] source/crates/campfire-client/src/view.rs:75 — `Drawn(Entity)` is cleared by a `Despawn` observer (:498), while gauges are found gone by polling (hud/mod.rs:383-385). Target: the relationship.
- [ ] source/crates/campfire-client/src/hud/gauge.rs:15 — `Gauge { unit }` points one way, so `mark_hits` (hud/mod.rs:274), `fill_gauges` (:316) and `place_gauges` (:378) scan every gauge every frame with random lookups. Target: gauges as anchor children; `place_gauges` goes; fills and hits from change filters.
- [ ] source/crates/campfire-client/src/hud/mod.rs:108 — the `Hud` and `View` chains (view.rs:256) both run in `Update`, unordered, so bars and rings may sit a frame behind the glide. Target: removed by propagation; the `Orders` pick, which reads drawing transforms, runs after the view's set.
- [ ] source/crates/campfire-client/src/hud/mod.rs:409 — `mark_target` assigns `Visibility` and `Transform` every frame (:409, :412-415). Target: the ring changes parent only when the target changes.
- [ ] source/crates/campfire-client/src/view.rs:390 — `lean` visits every drawn unit every frame, projectiles included. Target: the body child of attackers, and a `Changed<ActionSlots>` pass for the reset.
- [ ] source/crates/campfire-client/src/hud/mod.rs:279 — `mark_hits` scans every gauge in a mode with no life pool. Target: `.run_if(resource_exists::<Life>)` until the change filter replaces it.
- [ ] source/crates/campfire-client/src/hud/mod.rs:184 — `add_gauges` allocates a kinds `Vec` and a `ranks` `Vec` (:204) per new unit. Target: `Local` scratch.
- [ ] source/crates/campfire-client/src/orders.rs:29 — `click`, `cast` and `stop` build `Pointer` (two `Single`s and queries) every frame only to return early. Target: input run conditions.

## The client's drawings follow a clock that is not the sim's [medium]

Design: a drawing shows a state between two sim states, and the fraction comes from the clock that runs the sim. For a predicted unit, Lightyear's frame interpolation is the established practice: the drawing lerps between the last two ticks' states by `Time<Fixed>::overstep_fraction()`. That fraction is exact at any speed, at a pause, and after a rollback. A remote unit glides from where it is drawn to its new state over `Time<Fixed>::timestep()`, which is read each frame. Lightyear's interpolation timeline is the full practice for a remote unit, and it is a design step of its own.

- [ ] source/crates/campfire-client/src/view.rs:57 — `TickSeconds` is set once from the pin (:254), and each glide lasts that long (:465). The speed keys of a local match change `TickDuration` and `Time<Fixed>`, so drawings trail at 2× and 4×, and at 0.5× they stop and then jump. Target: the design; `TickSeconds` goes.
- [ ] source/crates/campfire-client/src/view.rs:84 — `Glide::since` and `Ring::since` (hud/ring/mod.rs:7) hold `elapsed_secs()` as `f32`, which steps 2 ms after 4.6 h. Target: the overstep fraction needs no timestamp; a timestamp that remains is a `Duration`, and only a difference becomes an `f32`.
- [ ] source/crates/campfire-client/src/view.rs:434 — `Dead` is predicted, so a rollback across a death yields the unit from both `Added<Dead>` and `RemovedComponents<Dead>` in one frame, and the chain applies them in event order, drawing a dead unit alive. Target: the drawing keeps the state it shows, and each touched unit is shown by its current `Has<Dead>`.

## Systems rescan every unit each tick where a change filter or a list would do [low]

Fixes in `campfire-capabilities`. A gate must only skip, never decide: when anything that could matter changed, the full pass runs as today, so a missed source costs time, never a wrong state.

- [ ] source/crates/campfire-capabilities/src/stats/mod.rs:139 — `(Refresh::give_parts, Refresh::run)` runs at `SimEdge::Start` and after each of the 9 stages. Each pass is an `Or<(Changed<…>, With<LiveShares>)>` scan, and `LiveShares` is a marker that `Commands` inserts and removes. That moves units between archetypes and adds two sync points to each of the 10 passes. Keep the passes, because removing the passes after the stages that do not write today breaks when a stage gains a writer. Target: the units with live shares are a sorted list on `Refresh`'s own resource, so no marker, no `Commands` and no sync point.
- [ ] source/crates/campfire-capabilities/src/navigation/mod.rs:249-267 — `track_static_bodies` (2× a tick, and at each box spawn) collects, sorts and compares every static body when none changed. Target: a skip when no `Position` or `Body` of a non-walker changed, no `MoveStep`, `Dead`, `Body` or `StableId` was added or removed, and nothing despawned; otherwise the full diff, unchanged.
- [ ] source/crates/campfire-capabilities/src/items/mod.rs:80 — `hold_items` (2× a tick) recomputes every inventory against every item modifier, with no change gate. Target: the units whose `Inventory` or `Modifiers` changed, or all of them when a book changed, as `hold_passives` does.
- [ ] source/crates/campfire-capabilities/src/production/gather_loop.rs:655 — `tag_gatherers` visits every `Gatherer` each tick, but its output depends only on `Gatherer`. Target: `Changed<Gatherer>`.

## Net systems build queries per frame, and its schedule chains what need not be ordered [low]

- [ ] source/crates/campfire-net/src/sim_server/door.rs:70 — `Door::take_joins` builds `world.query_filtered::<JoinLink, Unanswered>()` every frame (500 Hz). Target: a `Local<QueryState<…>>`, as `BotDriver` caches `Avatars`.
- [ ] source/crates/campfire-net/src/sim_server/checkpoints/mod.rs:136 — the same in `Checkpoints::take_commands`. Target: the same.
- [ ] source/crates/campfire-net/src/sim_client/mod.rs:204-220 — eleven mostly independent receive systems are `.chain()`ed with the exclusive `Faults::watch` in the middle, adding sync points every frame. Target: only the orderings that matter; `Faults::watch` last.
- [ ] source/crates/campfire-net/src/input_message.rs:140-151 — `InputMessage::new` grows `frames` and `payloads` by push from empty, though the caller knows both counts. Target: `with_capacity` from them.

## The server signs and then verifies its own inputs, allocating each time [medium]

- [ ] source/crates/campfire-protocol/src/session_log/mod.rs:784 — `record_server` always verifies the server's own Schnorr signature (~25.6 µs). Net's `ServerSigner::serve` signs and then records each `ServerInput::Bot`, so each bot input pays both on the tick thread. Target: `record_server` takes the signer and signs the message it already built. A signature the log made needs no check; a `debug_assert!` stays. Verification stays on decode and on replay.
- [ ] source/crates/campfire-protocol/src/server_input/mod.rs:183 — `message` builds a fresh `Vec` on each sign and verify, and its `+ 64` guess reallocates for a Join or Renew's delegation JSON. `Checkpoint::message` (checkpoint/mod.rs:96) and `SessionResult::message` (session_result/mod.rs:53) have the same shape. Target: one scratch buffer on the log, cleared and refilled, which all three use.

## A failed script call allocates and loses its case [medium]

- [ ] source/crates/campfire-script/src/script_host/error.rs:33 — `ScriptError::Runtime(error.to_string())` formats Rhai's message on every failed call, and the runner formats it again (`campfire-runner/src/session/mod.rs:297`). An AI script that fails for each unit allocates two times per unit per tick. Target: an enum of the cases callers tell apart, with the boxed Rhai error kept as it came. It is formatted only when it is logged.
- [ ] source/crates/campfire-script/src/script_host/error.rs:28,32,51 — `from_eval` takes `&EvalAltResult`, though `call` (mod.rs:195) owns the box. So the raised `Dynamic` is cloned, and `Raised::get` clones it again before `try_cast`. Target: `from_eval(Box<EvalAltResult>)` moves the value out, and `get` reads through `read_lock`.

## A match build repeats the load's work [low]

- [ ] source/crates/campfire-package/src/mode_packages.rs:179-194 with source/crates/campfire-runner/src/match_build.rs:50 — each match parses every script again, though the load parsed them (mode_packages.rs:385-386) with the same engine setup. The verifier builds a match for each checkpoint (`campfire-verifier/src/replay/mod.rs:78`). Target: the load keeps each `AST` in an `Arc` on its `Script`, and each match's host shares it. Together with the shared API modules, a match build parses nothing and binds nothing.
- [ ] source/crates/campfire-package/src/mode_packages.rs:206-237 — `book_input` recomputes `tag_names()` and `stat_graph().order()` for each match, and the load computes them two times (load_check.rs:85, :139, :142). Target: compute them once in `assemble`, and keep them on `ModePackages`.

## Untrusted package files are read without bounds and held whole [medium]

- [ ] source/crates/campfire-package/src/package_dir/mod.rs:110 and source/crates/campfire-package/src/package_store.rs:16 — `fs::read` takes files of any size and any count. `PackageStore` keeps every byte of every package for the life of the process, assets included, though the load reads only the manifest, `data/`, `map/`, `scripts/` and `locale/`. Target: one walk per package that checks the per-file and per-package limits from metadata before it reads. It reads the files the load needs, and it hashes the other files as a stream for the fingerprint. The store keeps only the files the load reads.

## A superseded link ends after one send of its notice [medium]

- [ ] source/crates/campfire-net/src/sim_server/offering.rs:73-100 — `tell` queues `Superseded` in frame N, and `end` unlinks in frame N+1, after the first send. So the reliable channel never sends it again. If that packet is lost, the old client rejoins and supersedes the newer login. That is the ping-pong that `Superseded` is there to stop. Target: the link ends when the message is acknowledged, or after a timeout that allows a resend.

## The LAN check leaks its child processes [medium]

- [ ] source/checks/campfire-lan-check/src/lan_match.rs:97 — after the server starts, each `?` in `play` (:97, :124, :127, :129-144, :150) drops the `Child` handles with no kill. Target: a guard type that owns every child, and kills and waits on each one on drop.
- [ ] source/checks/campfire-lan-check/src/lan_match.rs:338 — `free_port` reports a failed probe bind as `CheckError::Start { process: Server }`, which blames a server that never started. Target: a case of its own.

## Guards that do not guard, and core resources that a restore can drop [low]

- [ ] source/crates/campfire-script/src/script_host/mod.rs:79-80 — `set_hashing_seed` is `OnceCell::set`. Its `Err` holds the value just passed, not the stored seed, so the `assert_eq!` never fires. Target: after the set, assert `hashing::get_hashing_seed() == &Some(HASHING_SEED)`. This becomes a precondition of the shared API modules, whose function hashes use the seed.
- [ ] source/crates/campfire-script/src/script_host/mod.rs:78,103 — `ScriptHost::new(per_call: u64)` passes 0 to `set_max_operations`, and Rhai reads 0 as no limit. Target: take `NonZeroU64`.
- [ ] source/crates/campfire-sim/src/state_registry/mod.rs:507 — `decode_resource` reads `None` as "remove", and `check_resource` (:614) lets the resource be absent. So a snapshot without `sim.tick` restores, and then `start_tick` panics. Target: the sim's own resources are required, and a `None` section for one of them is an error case.
- [ ] source/crates/campfire-sim/src/sim_update/mod.rs:101 — `start_tick` and `end_tick` are not ordered around `SimEdge::Start` and `SimEdge::After(SimSet::Vision)`, so an edge system that reads `SimRng` or `TickInputs` cannot be ordered against them (`stage_clock.rs:236,250` works around this). Target: `start_tick.before(SimEdge::Start)` and `end_tick.after(SimEdge::After(SimSet::Vision))`.

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
- [ ] source/crates/campfire-client/src/view.rs:75 — `Drawn`, `Glide`, `Look` and `Footing` are separate crate-visible types, but they sit in the `View` plugin's file. `View::float` (:279) is a general `Num` to `f32` conversion, but it is a function of the plugin. Target: a file for each type, and the conversion on a type that owns it. The hierarchy redesign moves most of these types.
- [ ] source/crates/campfire-client/src/args/mod.rs:13 — `pub(crate) mod error;` is used only in `args`, and the same is true of `server_tls/mod.rs:12`. Target: private.
- [ ] source/crates/campfire-server/src/server_config.rs:7 — `ServerConfig(ServerSetup)` is always inserted (main.rs:123), but only `Restore::run` (opening/mod.rs:146) reads it. Target: the setup goes into `Restore`, and `ServerConfig` goes.
- [ ] source/crates/campfire-net/src/sim_server/bot_driver.rs:17 — the server imports `crate::sim_client::bot_script::BotScript`. Target: `BotScript` goes beside `order_script`.
- [ ] source/crates/campfire-package/src/mode_packages.rs:127 — `pub const fn mode()` has no caller. `stat_graph` (:287) and `slotted_ranks` (:313) are `pub`, but only the crate calls them. Target: delete `mode()`, and make the two `pub(crate)`.
- [ ] source/crates/campfire-runner/src/input_rules.rs:24 — `#[cfg(feature = "internals")] pub const ROOMY` is gated in the middle of an `impl`. Target: the gated `internals` module at the end of the file.
- [ ] source/crates/campfire-script/src/script_host/mod.rs:45 — `ScriptId::nth` can be a `const fn`. The same is true of `ground_offset` and `within_ground` in `campfire-sim/src/position.rs:38,45`, through `Vec3::checked_sub`.
- [ ] source/crates/campfire-common/src/lib.rs:6 — the crate doc says the crate depends only on `serde`, but `Cargo.toml` also has `derive_more`. Target: make the two agree.

## Design and code disagree [low]

- [ ] source/crates/campfire-runner/src/session/error.rs:24,34 — `02-engine-core.md:103` says `StartError` has no data case. But it has `MatchStart(CallError)`, an `on_match_start` failure that the load cannot refuse, and `Packages(StoreError)`. Correct one of the two, and state which.
