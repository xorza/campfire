# Review: the workspace

Scope: Bevy ECS use in `sim`, `capabilities/*` and the client; `session/*` and the protocol; `network/*`; the import and `base/*`; the test harnesses, `checks/*` and the driver's config; and the agents' sessions of 2026-09-30 to 2026-10-24 (25 transcripts). Line numbers are those of `adc5a0777`. Each finding below was read in the code.

## Proposal

The code is careful where it is local: parsers are bounded, scratch lives in `Local`, passes are mostly change-driven and ordered by stable id, and canonical encodings are enforced. The defects come from six root causes. Each section states the cause, then the shape that removes it, as a type or a declaration where one can hold it.

### R1. A bound is held by an assumption, not by a type

A limit that the design promises holds only while another invariant holds, and a later change broke it with no test to see it.

- **Late inputs have no window** (a regression of `7372a5bcf`). `InputSchedule::schedule` takes a late input to the next free place of its player slot's `Spill`, with no check against `max_lead`. The only limit is the stamp total since the join, which grows by `max_per_tick` for each idle tick. After T idle ticks a client can queue 4·(T+31) inputs, which apply up to T ticks ahead and grow the next checkpoint's carry. `SessionTerms::largest_record` sizes the carry by a window of `max_lead + 1` ticks, and the terms are refused when that passes the 16 MiB record bound; so the bound is real only while the window is. Past it, a release build writes a frame that a restore reads as torn, and the session after it is lost with no error. The bot path already refuses a place past `next + max_lead`; the player path does not.
  **Target:** one window for every input, held by the type of a place, and a client that never reaches it. An input applies at its place, the first tick from `max(stamp, next)` on that its player slot's inputs do not fill; a place must be within `max_lead` of `next`. `Spill::place` returns a `Place`, `InputWindow::admit(Place) -> Option<Admitted>` is the one check, and `Spill::take` takes only an `Admitted`, so no code can queue past the window. A packet with an input that `admit` refuses is refused whole as `AheadOfTime`, which unlinks, as a flood is now; a bot input is dropped and counted, as now. So every logged input applies, as Factorio's server promises ("Input Actions are never ignored, they are only delayed", FFF #302) and 05-protocol-spec.md, A late input applies at the next tick, decides. The client keeps its pending inputs, those not acknowledged and those acknowledged for a tick still to come, and holds a new input while they fill the window, so a correct client is never refused. `CheckedCarry::holds` checks the same window. The stamp total stays: it bounds the rate at which a player grows the log, which the window does not.
- **A reliable channel drops what it cannot send.** `MessageSender::send` cannot fail; at the flush, `SendError::Backlogged` and `SendError::MessageTooLarge` are logged and the message dropped, and the ones queued behind it are sent. On an ordered reliable channel, and on replicon's update stream, the peer then holds a stream with a hole and diverges for good. The idle timeout that `transport.md` relies on never fires for a slow peer that still acknowledges. renet disconnects in this case; GameNetworkingSockets returns the error to the sender.
  **Target:** the transport owns the consequence, so no caller can drop. On a reliable channel, a send it cannot take unlinks the link with a typed `UnlinkReason` (`Backlogged`, `MessageTooLarge`); on an unreliable channel it is dropped and counted on the link. `SendError` then keeps only `ChannelNotFound`, a logic error that panics. A broadcast goes on past a link that unlinks.
- **`KnownCommanded.units` grows for the whole session** while a select is always in flight, as it is cleared only when none waits. The acks come in seq order, so the waiting selects are a queue.
  **Target:** a `SelectQueue` that owns the flat buffer of the waiting selects' units and drops a select's units as its ack pops it, so the buffer holds exactly the units of the selects that wait.
- **The package fingerprint is taken from the re-encoding**, not from the bytes read; the two are compared only in a `debug_assert_eq!`. One lenient field parser would make a release build fingerprint bytes nobody holds.
  **Target:** `PackageFingerprint::of(bytes)` on the input, and a release comparison of the bytes with the re-encoding, which refuses a non-canonical index.
- **A stale doc of the last batch.** `Bounty::pay` (`mode/src/bounty_book.rs:103-107`) says "a payment past what a resource holds is not made", but `PlayerResources::gain` now saturates (04-capabilities/games.md, A gain stops at the most an amount holds).

### R2. A tick's faults are checked by each test driver, not by the match

Each report a tick writes is declared once (`Declarations::report`), but what a test does with it is decided by each driver. Unread commands are asserted in three places (`SimWorld::step`, `FixedMatch::step`, `HashTrail::record_read`); a failed script call fails a test only where it asserts `failed_calls` by hand; and `ScriptPrints` reach a log only in the network server. So the `Match::builder` tests, the `Arena` tests, the network tests' live server and the resumed matches each miss some checks, which is the "can be forgotten" shape the Direction refuses. Agents debugging the Zero Hour check turned a script's `print` into `throw` to see a value, and logs below Warn never reach a failing test.

**Target:** a report states what it is, as a trait item with no default: `Report::KIND: ReportKind`, `Record` for what the host reads (deaths, budgets, saves asked, dropped orders), `Fault` for what a correct match never writes (unread commands, script failures). Under `internals`, the declaration walk installs one system at the tick's end that moves each fault report's entries into `UntakenFaults`. The next tick's start panics on faults still there, and `UntakenFaults`'s drop panics when the world drops with any, unless the thread already panics. A test that expects a fault takes it by `UntakenFaults::take::<R>()`. The check is in the schedule, so every driver has it: `SimWorld::step`, `run_session_tick`, the network server, every replay. The three assert sites, `step_unread` and the hand asserts of an empty `failed_calls` go. The runner's `run_session_tick`, which already logs script failures, logs `ScriptPrints` too, and the server's own site goes.

### R3. The dev loop throws its signals away

In the `cargo drive` era agents waited on builds and tests 23.1 h of 69.5 h; 42 % of `check` runs and 40 % of `test` runs failed. Most of the loss is reruns to learn what failed:

- The verdict shows one error, so a rename takes one run per error: of 235 FAIL→FAIL pairs, 223 showed a new first error.
- A failed test's verdict names it but not its panic: agents grepped `panicked` in `tests.log` about 200 times.
- The Zero Hour check's "first error" is its first INFO line in 29 of 46 failures.
- A compile error of the test binaries shows as `FAIL index`, the step that builds them first.
- A timeout under load reads as a failure; one cost about 1 h of gate reruns.
- Every full run reports 30–45 SLOW tests, and `junit.xml`, overwritten by each run, keeps none of their names.
- `checks/vocabulary` is a stale fork of the driver's own `vocabulary` command, 88 lines of fixes behind; its one addition is a `NamedEnum` derive.
- No lane builds the game to play, so agents ran `cargo build -p … --profile release-dev` 15 times, the only standing breach of the rule against `-p`.

**Target:** the verdict carries what the fix needs. In the driver: a count and the first error in the verdict, and every deduplicated `file:line: message [lint]` in `last-<command>.txt`; each failed test's panic location and first lines; a command's error pattern from its table; a TIMEOUT of its own; the build of the test binaries as a step of its own; the full run's SLOW lines kept with their names. In this repository: `checks/vocabulary` goes, and the commands table gains `client` and `server`. The driver's items belong to the `drive` repository.

### R4. A sim mechanism stops short of its plumbing

- **Timed processes.** `TimedProcess::due` states when a unit is due, but for each process kept `BySetters`, the arrival of its part is a hand-written hook (nine: `areas/src/area.rs:51`, `combat/src/respawn.rs:23`, `navigation/src/route.rs:104`, `production/src/kept_spawns.rs:240`, `production/src/train_queue.rs:94`, `production/src/order_dues.rs:75`, `orders/src/next_turn.rs:76`, `stats/src/modifiers/modifier_clocks.rs:59`, `stats/src/modifiers/mod.rs:102`), and each setter goes through a hand-written handle of the same shape (five: `RouteDues`, `GoalDues`, `OrderDues`, `QueueDues`, `ModifierDues`).
  **Target:** `TimedProcess` gains `type Arrives: Component`, the part whose insert gives a unit its time, with no default; `DueIndex::track` installs one generic insert observer beside its `predicted` one, which reads `P::Parts` and `P::Books` and pushes `P::due`. One `UnitDues<'_, P>` replaces the five handles.
- **Named ids.** The match's and the players' entities are found through `EntityIndex`'s B-tree on every call (`OnMatch::get`, `PlayerRows::of_player_slot`, `UnitsWorld::relations`), per candidate in `orders/src/acquiring.rs:113-116` and per row in `units/src/row_source.rs:108`. PERF.md measured `OnMatch<Relations>::get` at 10.5 % of `gather/two_fields`, and fixed it only inside the gather loop. The named ids are a fixed set: `StableId::MATCH` and one per player slot after it.
  **Target:** `EntityIndex` keeps the named ids' entities in an array by name beside the B-tree of allocated ids, so a named lookup is O(1) for every caller with no API change; its iteration stays in id order, as every named id is past every allocated one.
- **The radius scan.** "Units of a filter within a radius" is written five times (`combat/src/attacks.rs:132`, `areas/src/area_life.rs:92`, `stats/src/held_pass.rs:155`, `progression/src/experience_book.rs:229`, `script/.../target_index.rs:106`), each with its own living check and reach test, although 04-capabilities/00-overview.md, One reach rule, says one rule decides.
  **Target:** one `Targets::within(point, radius, team, filter, Measure, &mut Vec)` in `actions`, where `Measure` names the reach of a body or of a volume; a copy that differed is a decision to check, not a silent change.
- **The gather loop reads its own writes as changes.** `GatherLoop::run` takes `Changed<Gatherer>` through a `SystemState` whose last run is before its own `Gatherer::set_in` writes, so the next tick sees them and `free_left_nodes`, a scan of every node with a sort, runs after any worker stepped, against its doc.
  **Target:** a worker that leaves its node says so where it leaves: `Gatherer::set`, which every writer calls, writes a `LeftNode` message, and `free_left_nodes` reads the messages and `Removals<Gatherer>`, with no change tick to infer from.

### R5. I/O and work traded for memory, and cold paths that run serially

- Publishing walks the journal twice through an unbuffered `File`, two `read_exact` calls per frame, to recount tick counts and segments that the live log had and forgot. On an abort it reads a third time what `restore` already holds.
- Restore re-verifies every Schnorr signature since tick 0 over the server's own hashed journal, while players wait; a write-ahead log (PostgreSQL, Raft) trusts its own checksums on replay.
- `Session` holds its role as two `Option`s, `signer` and `journal`, and `Self::server(...).expect` panics when a verifier's session is asked to serve.
- `PackageStore::scan` decodes each index and drops it; `find` reads and decodes it again.
- The Zero Hour import (15.0 s of a 27.3 s check run, `docs/research/debug-loop.md`) runs on one thread, although its output order is fixed by `BTreeMap` keys; `threads::Parallel` exists only under `internals`. The install's 32 archives are hashed in 1 MiB chunks, each read into a new zeroed `Vec` with an `fstat`.
- `DurableDir::create_all` asks `Path::is_dir`, which `clippy.toml` bans and 02-engine-core.md, Bounded reads, no existence checks, refuses; `PackageWriter::write` calls it, with a directory sync, per file.

**Target:** the log keeps its shape live, and publish walks the journal once through a buffer. Two constructors in place of one with a flag: `SessionLog::restore_own(journal)`, which trusts the journal's frame hashes, and the verifier's checked load. `enum SessionRole { Server { signer, journal: Option<Journal> }, Verifier }`. `StoredPackage` keeps its `Arc<FileIndex>`. `threads` gains an ordered parallel map with a bounded window in production, and the import converts and hashes through it, then writes in key order through one `PackageWriter`. `store` gains `InputRanges::read_at_into`. `create_all` follows std's `create_dir_all`: create, and on `NotFound` create the parent first.

### R6. The packet layer assumes about one packet per frame each way, and prediction scans everything

An ACK names the largest number and the 32 below it, packets are accepted once per frame, and more than 64 in a frame are dropped. When a burst puts more than 33 packets between two of the receiver's frames, the oldest are never acknowledged, so they are declared lost, sent again, and the congestion window halves. The one-byte ACK delay saturates at 63.75 ms, so a slow frame inflates the RTT sample that sets input delay. On the client, the rollback scan visits every predicted entity for each component type on every confirmed tick, and writes an anchor into each history. Both are claims from reading the code, with no case that shows their cost.

**Target:** measure first, with a net-emulator case of a burst past 33 packets in one receiver frame and a bench of the rollback scan over idle units. Then one window of 64 for the ACK bits, the replay window and the packets a frame accepts, a single constant, so an ACK names every packet a frame accepted; and an ACK-only packet at the end of a frame whose unacknowledged packets pass half the window, as QUIC acknowledges every second packet (RFC 9000 §13.2.2; quinn's `ack_eliciting_threshold` of 1). RFC 4303 §3.4.3 prefers a window of 64 and lets a receiver take a larger one. Then an ACK delay that cannot saturate silently, and a scan over the pairs that gained an entry.

### Decisions taken

- **An input past the window is refused, and the client holds its inputs so it never sends one** (batch 1). Refused: an input logged that never applies, which 05-protocol-spec.md, A late input applies at the next tick, refuses; a refusal with no client guard, which unlinks a scripted client that sends 4 inputs a tick through a stall.
- **One window of 64 for the ACK bits, the replay window and a frame's accepted packets, with an ACK-only packet past half of it** (batch 9, once its burst case shows the losses). The header grows from 7 to 11 bytes. Refused: ACK ranges, as QUIC and GameNetworkingSockets send them, more code for bursts a frame's accept already bounds at 64; a cap of 33 packets in flight, which bounds the throughput at 33 packets a receiver frame.

## Plan

### Rules for every batch

- **A bug's test fails first.** Each bug item names the test that fails before its fix and passes after it.
- **A performance item is measured.** It names its bench case. The change stays only when the median of interleaved pairs moves past the case's noise floor, by the method of `.notes/PERF.md`. A case that does not exist is added first, in its own commit.
- **A design decision that changes is written in the same batch,** in its system's document, with the source the item names, and its citations in code follow.
- **Each batch says whether goldens move.** A golden that moves names why; one that moves in a batch that says none is a defect of the batch.
- Each batch ends with `cargo drive gate` green and a stop for review. Whoever addresses an item deletes it.

### Batch 1: bounds that hold (R1)

Files: `session/protocol/src/{spill.rs,session_log/{mod.rs,input_schedule.rs,checked_carry.rs},session_terms.rs}`, `session/host/src/client/{known_commanded,sent_inputs}/mod.rs`, `network/net/src/{sim_server,sim_client}/mod.rs`, `session/package-format/src/file_index/mod.rs`, `capabilities/mode/src/bounty_book.rs`, `docs/design/05-protocol-spec.md`.
Goldens: the log tag moves to v13, as the rule a log is checked by changes, and the pinned logs move by their tag alone.

- [ ] `spill.rs:14-34`, `input_schedule.rs:65-76`: a late input queued past `max_lead` → `Spill::place` returns `Place`; `InputWindow::admit(Place) -> Option<Admitted>`; `Spill::take(Admitted)`.
- [ ] `session_log/mod.rs:604-623` (`record`): a packet's inputs placed with no window → each placed through `admit` before the packet is taken; one refused makes the packet `AheadOfTime`. The bot path at `:714-721` uses the same `admit`.
- [ ] `sent_inputs/mod.rs`: the client sends whatever its player gives → `SentInputs::pending()`, the inputs not acknowledged and those acknowledged for a tick still to come; the client holds a new input while they fill `(max_lead + 1) × max_per_tick`, and sends it as they apply.
- [ ] `checked_carry.rs:106-128` (`holds`): no check of the window → a carried input applies within `max_lead` of the record's tick.
- [ ] Test, fails first: a player idle for 1,000 ticks sends `max_per_tick` late inputs of the largest payload each tick; the packet that would pass the window is refused as `AheadOfTime`, its pending inputs stay at most `(max_lead + 1) × max_per_tick`, and a checkpoint's carry at most `largest_record`.
- [ ] Test: a client whose link stalls for 100 ticks while its player gives 4 inputs a tick is never refused, and every input applies, in order.
- [ ] `05-protocol-spec.md`: A late input applies at the next tick states the window, the refusal past it and the client's hold, with FFF #302 as a source; the claim of 126,976 bytes held per player follows from it.
- [ ] `known_commanded/mod.rs:58-101`: units cleared only when nothing waits → `SelectQueue`, which drops each select's units as its ack pops it. Test, fails first: 100 selects, each acknowledged while the next is in flight, leave the buffer at one select's units.
- [ ] `file_index/mod.rs:93-122` (`decode`): fingerprint of the re-encoding → of the input bytes, and a release refusal of an index whose bytes differ from its re-encoding. Test: an index with a non-canonical field is refused.
- [ ] `bounty_book.rs:103-107`: the doc says a payment past the most is not made → it stops at the most, as `gain` does.

### Batch 2: a reliable channel never drops (R1)

Files: `network/net-channels/src/{channels/mod.rs,error.rs}`, `network/net-messages/src/{send_queue.rs,multi_message_sender/mod.rs}`, `network/net-replication/src/{server_replicon_plugin,client_replicon_plugin}/mod.rs`, `network/net-link` (`UnlinkReason`), `docs/design/11-net-stack/transport.md`.
Goldens: none.

- [ ] `channels/mod.rs:397-410` (`check_send`): a refusal returned for the caller to log → on a reliable channel the transport unlinks with `UnlinkReason::Backlogged` or `MessageTooLarge`; on an unreliable channel it drops and counts on the link.
- [ ] `error.rs:46-55`: `SendError` keeps `ChannelNotFound` alone; `send_queue.rs:129-150`, `server_replicon_plugin/mod.rs:253`, `client_replicon_plugin/mod.rs:165` lose their log-and-drop arms.
- [ ] `multi_message_sender/mod.rs:123`: `?` stops a broadcast at the first refusal → it goes on to every link.
- [ ] Test, fails first: a link under a `BandwidthLimit` that still acknowledges is sent past its budget on a reliable channel; it unlinks with `Backlogged`, and no message after a hole reaches its peer.
- [ ] `transport.md`, A link's queued bytes have a budget: the refusal of an unlink at the budget → the unlink, with renet's `ReliableChannelMaxMemoryReached` and GameNetworkingSockets' `k_EResultLimitExceeded` as sources.

### Batch 3: a tick's faults fail the test in every tier (R2)

Files: `capabilities/core/declarations/src/{report.rs,declarations.rs}` and the walk that installs reports, the six `impl Report` sites, `sim/sim/src/{sim_world.rs,unread_commands.rs}`, `test/harness/src/{fixed_session/fixed_match.rs,hash_trail.rs}`, `session/runner/src/session/session_world.rs`, `session/runner/src/events` (`ScriptPrinted` moves here from `host`), `network/net/src/sim_server/mod.rs`, `base/log/src/json_layer.rs`, `checks/zero-hour-check/src/match_play.rs`, the tests that assert `failed_calls` or take unread commands.
Goldens: none.

- [ ] `report.rs:9-13`: `Report` gains `const KIND: ReportKind` with no default; `UnreadCommands` and `ScriptFailures` are `Fault`, the others `Record`.
- [ ] The declaration walk, under `internals`: one tick-end system that moves each fault report's entries into `UntakenFaults`; the next tick's start panics on faults left; its drop panics when the world drops with any.
- [ ] Risk to check first: a predicting client's role may count commands it does not read (the mode's, where it holds no `MatchMode`). A fault report whose role can write it on a correct match is not a fault for that role; `Report::installed` already sees the role.
- [ ] `sim_world.rs:90-100`, `fixed_match.rs:70-90`, `hash_trail.rs:56`: three assert sites → gone; `step_unread`, `step_with_unread` and `tick_inputs_unread` → `UntakenFaults::take::<UnreadCommands>()`.
- [ ] The hand asserts of an empty `failed_calls` (44 matches of `failed_calls(&`) → gone; a test that expects a failure takes it.
- [ ] Test, fails first: a `Match::builder` match, a network test's live server and a resumed match each panic on a script that throws in a hook.
- [ ] `session_world.rs:142-154` (`run_session_tick`): logs script failures, not prints → logs `ScriptPrints` too; the server's site goes.
- [ ] `runner/tests/resume.rs`, `verifier/tests/session_logs.rs`: five hand-built `SessionSource::check(UncheckedSource {…})` and bare `run_tick_alone` → `FixedSession::source(log)` and `FixedSession::resume(log, at) -> FixedMatch`.
- [ ] `json_layer.rs:95-118` (`LogCheck::start`): only Warn and above reach a test → a stderr layer filtered by `RUST_LOG`, installed only when the variable is set; AGENTS.md's Checks names it.

### Batch 4: the dev loop keeps its signals (R3)

Files: the `drive` repository; `source/Cargo.toml` `[workspace.metadata.drive]`; `checks/vocabulary` (deleted); `store/src/pinned_file.rs`; `harness/tests/runner/snapshot_layout.rs`; `AGENTS.md`; `docs/design/02-engine-core.md`.
Goldens: none. The driver's items ship as a driver release, and the table's requirement follows it.

- [ ] Driver: the verdict shows one error → a count and the first error in the verdict, every deduplicated `file:line: message [lint]` in `last-<command>.txt`.
- [ ] Driver: a test FAIL names the test only → each failed test's panic location and its first lines, up to five tests.
- [ ] Driver: a command's first error is its log's first line → an `error` pattern per command in its table; the Zero Hour check's is `ERROR|panicked at|error\[`.
- [ ] Driver: a timeout reads as a FAIL → a TIMEOUT of its own in the verdict.
- [ ] Driver: a compile error of the test binaries shows as `FAIL index` → the build is a step of its own.
- [ ] Driver: the slow list is lost with `junit.xml` → the full run's SLOW lines, with their names, in its run log.
- [ ] `checks/vocabulary` → delete; `cargo drive vocabulary` replaces it in the commands table, AGENTS.md's Checks and 02-engine-core.md.
- [ ] No lane builds the game to play → `commands.client` and `commands.server`, built in `release-dev`.
- [ ] `pinned_file.rs` (`PinnedFile::pin`): says only "differs" → names the first differing line; the layout test's panic names `StateRegistry::DATA_VERSION`.
- [ ] A bless took two runs where a test reads a file that another test pins in the same run (`d1ea3e72`, 2026-10-21T16:03) → find that pair; each test computes its own inputs.
- [ ] `AGENTS.md`: benches take a median 444 s, and three passed the tool's 600 s limit → `cargo drive bench` runs in the background.

### Batch 5: the sim's mechanisms reach their plumbing (R4)

Files: `sim/sim/src/{due_index.rs,timed_process.rs,stable_id/entity_index.rs}`, the nine hook sites and five handle sites of R4, `capabilities/core/actions/src/targets.rs`, the five radius-scan sites, `production/src/{gather_loop/mod.rs,gatherer.rs}`.
Goldens: none, but where a radius-scan copy differed from the rule; each such difference is named in the commit.

- [ ] `timed_process.rs:10-29`: `type Arrives: Component`, no default; `due_index.rs` `track` installs the generic insert observer; the nine hooks go. Check first that each hook's component is the one its `Parts` read.
- [ ] Five `*Dues` handles → one `UnitDues<'_, P>`.
- [ ] `entity_index.rs:11-21`: named ids in the B-tree → an array by name beside it. Bench: `gather/two_fields` and `server_tick/mean_3v3`.
- [ ] Five radius scans → `Targets::within` with a `Measure`.
- [ ] `gather_loop/mod.rs:158-165`: own writes read as changes → `LeftNode` messages from `Gatherer::set`, and `Removals<Gatherer>`. Check first that every writer of a gather order goes through `set`. Bench: `gather/two_fields`.

### Batch 6: the session log reads once (R5)

Files: `session/protocol/src/session_log/{publish.rs,journal/journal_frames.rs,journal_records.rs,mod.rs,checkpoint/mod.rs,checked_carry.rs}`, `session/host/src/server/session_dir/mod.rs`, `session/package-disk/src/package_store.rs`, `session/runner/src/session/mod.rs`.
Goldens: none; a published log's bytes stay the same, which the verifier's tests show.

- [ ] `journal_frames.rs:62-89` (`stream`): two unbuffered reads per frame → a 64 KiB buffer.
- [ ] `publish.rs:98-160`, `session_dir/mod.rs:241-289`: two walks to recount the shape → the log keeps its per-tick counts, segments and cuts live; publish walks once; an abort publishes from the bytes `restore` holds.
- [ ] `journal_records.rs:83-115`, `session_dir/mod.rs:197`: restore re-verifies every signature → `SessionLog::restore_own(journal)`, which trusts the frame hashes. Bench first: a restore of an hour's journal of six players.
- [ ] `session/mod.rs:51-60,296-298`: two `Option`s and an `expect` → `SessionRole`.
- [ ] `mod.rs` `serve_checkpoint`/`take_checkpoint`/`serve_result`, `checkpoint/mod.rs:38-48`, `checked_carry.rs:65`: a record encoded twice and a cloned carry → encode once, as `EncodedPlace` does, and take the carry.
- [ ] `package_store.rs:62-112`: an index decoded twice → `StoredPackage` keeps an `Arc<FileIndex>`.
- [ ] `mod.rs:353` (`applied_inputs`): a `pub` fn that only `net-harness` calls → under `internals`.

### Batch 7: the passes that a measurement shows

Each item is a candidate; it goes in only as the rules for every batch allow.
Files: by item.

- [ ] No tick-level allocation audit, against 02-engine-core.md, Allocation audits → a `tests/alloc` over a running 3v3 match's ticks after warmup; what it shows decides the `DueIndex` items below.
- [ ] `due_index.rs:30-35`: B-tree nodes allocated as entries churn; `progress_dues.rs:47-91`: sets held at `Tick::ZERO` in a due index → structures that keep their buffers, if the audit shows them.
- [ ] `progress_dues.rs:252-275`: four nested `resource_scope` per call, each two archetype moves of the resource's entity and a flush in Bevy 0.20 (`bevy_ecs-0.20.0/src/world/mod.rs:2894-2960`), per channelling unit, cast and AI order → if `server_tick/mean_3v3`'s profile shows them, the four indexes reached once.
- [ ] `mode/src/income_clock.rs:121`: every income unit stepped each tick → a `TimedProcess`.
- [ ] `mode/src/victory.rs:104-129`: every living unit counted, sorted and deduped each tick → a per-player tally kept by observers and `Removals`, as `production/src/holdings.rs` keeps one.
- [ ] `combat/src/dying.rs:78`: every dead unit not despawning visited each tick → `Added<Dead>` and the removal of `Kept`.
- [ ] `navigation/src/room_making.rs:58-67`: every walker scanned per static spawn → the unit grid near the body's bound; case: a map load with its placed structures.
- [ ] `bots/src/bot_view.rs:243-268` (`nearest_where`): every world unit per order → the unit grid in growing rings; case: the Zero Hour check's base play, profiled.

### Batch 8: the import runs in parallel (R5)

Files: `base/threads/src/{lib.rs,parallel/mod.rs}`, `apps/import/src/zero_hour/{mod.rs,install/mod.rs,format/ini/ini_text.rs,format/ini/object_ini/mod.rs}`, `apps/import/src/texture/mip_chain.rs`, `base/store/src/{input_file/mod.rs,durable_dir/mod.rs}`, `session/package-disk/src/package_writer/mod.rs`, `base/platform/src/positional_read.rs`.
Goldens: none, but the mip chain's item, which moves every texture's bytes and the package hashes, in a commit of its own.

- [ ] Profile the import of the Zero Hour check's install first; the profile orders the items below and drops one it does not show.
- [ ] `threads/src/lib.rs:12-13`: `Parallel` only under `internals` → an ordered parallel map with a bounded window in production, so memory stays bounded and the output order is the input's.
- [ ] `zero_hour/mod.rs:132-305` (`ZeroHour::write`): every conversion in turn → models, textures and maps through the ordered map, written in key order. Test: the package's `package.index` stays the same (`zero_hour/tests.rs:104`).
- [ ] `install/mod.rs:259-279`, `input_file/mod.rs:194-214`: a new zeroed `Vec` and an `fstat` per 1 MiB → `read_at_into` with one buffer and one `unchanged()` per archive; archives hashed through the ordered map.
- [ ] `durable_dir/mod.rs:47-57`: `Path::is_dir` against the ban → create, and on `NotFound` create the parent first.
- [ ] `package_writer/mod.rs:44-46`: `create_all` and a directory sync per file → the writer keeps the directories it made.
- [ ] `install/mod.rs:180-254`: full scans of a sorted map and the language resolved twice → `range(prefix..)` with `take_while`, the language once at `Install::open`.
- [ ] `object_ini/mod.rs:31,288,344-346`: a `OnceLock` emptied on each read → resolved once after `read_inis`, an immutable value.
- [ ] `ini_text.rs:72-84,129-131,306-322`: a `Vec` and a lowercase `String` per line → keywords compared with `eq_ignore_ascii_case`, tokens as ranges, if the profile shows it.
- [ ] `mip_chain.rs:41-66`: each level rounds again from the level above → each level from the linear sums of the one above, rounded once, as the coding guide asks exact over approximate.
- [ ] `positional_read.rs:6-7`: promises no cursor move, which Windows' `seek_read` breaks → the read does not depend on the cursor.

### Batch 9: the packet layer under bursts (R6)

Files: `network/net-emulator` (a case), `network/net-packets/src/{received_window.rs,header.rs,packet_session/mod.rs,sent_packets/mod.rs}`, `network/net-payload/src/receive_payload/mod.rs`, `network/net-messages/src/{app_event_ext.rs,message_receiver/mod.rs}`, `network/net-replication/src/hello_plugin/mod.rs`, `network/net-sync/src/rtt_estimator_ewma.rs`, `network/net-channels/src/sent_packets.rs`, `network/net-prediction/src/rollback_plugin/mod.rs`, `docs/design/11-net-stack/packet-session.md`.
Goldens: the wire cost case's counts move, as each header grows by 4 bytes.

- [ ] No case shows a burst past 33 packets in one receiver frame → add it, with the false losses it counts. If it shows none, the two items after it go.
- [ ] `received_window.rs:15,20`, `header.rs:80-84`: `BITS = 32` and `PENDING = 64` apart, `bits: u32` → one `WINDOW = 64`, `bits: u64`; packet-session.md, One window of 32 accepted packets is both the ACK bits and the replay window, becomes one window of 64, with RFC 4303 §3.4.3's preference as its source. The burst case shows no false loss.
- [ ] `packet_session/mod.rs`: an ACK-only packet waits 25 ms whatever the frame accepted → it goes at the frame's end when the accepted packets no ACK named pass `WINDOW / 2`, as RFC 9000 §13.2.2 acknowledges every second packet. Test: two frames of 40 packets each, with no packet of the receiver's own between them, leave no packet unnamed.
- [ ] `header.rs:87`: the ACK delay saturates at 63.75 ms → a sample whose delay saturated is not taken; test: a receiver frame of 100 ms leaves the RTT estimate unmoved.
- [ ] `sent_packets/mod.rs:261` (`probe_deadline`): a scan of every packet in flight per schedule → a count of unsettled packets; bench: `udp_server`.
- [ ] `receive_payload/mod.rs:18`: `Frozen` and `into_mut` are dead → `OpenedPayload.payload: BytesMut`.
- [ ] `app_event_ext.rs:24`, `hello_plugin/mod.rs:31`: a second encoding for `SenderHello`, against api.md, One encoding of a message and a component → through the codec; the method goes.
- [ ] `net-channels/src/sent_packets.rs` holds `UnsettledPackets` → its own file.
- [ ] Docs: `rtt_estimator_ewma.rs:10-12` cites RFC 6298 for gains it does not use → time.md's tuning; `packet_session/mod.rs:207-210` puts `hand_over`'s doc on `has_inbox`; `message_receiver/mod.rs` says "deserialize" of decoded messages; `rollback_plugin/mod.rs:1-38,260` is a narrative and cites an issue by number.

### Batch 10: prediction visits what changed (R6)

Files: `network/net-prediction/src/{rollback_scan.rs,prediction_history/mod.rs,bench.rs}`.
Goldens: none.

- [ ] No bench of the scan → `prediction/scan/<n>_idle` over idle predicted units.
- [ ] `rollback_scan.rs:147-185`: every predicted entity × component type per confirmed tick, with an anchor written each → the pairs that gained a confirmed or predicted entry since they last matched.
- [ ] `prediction_history/mod.rs:206-247`: every history pruned per frame → pruned as an entry is added.

### Batch 11: one fixture per tier

Files: `capabilities/capabilities/src/fixtures/package_set.rs`, `session/runner/src/arena.rs`, `capabilities/capabilities/tests/{mode,roles.rs,feature_installs.rs,prediction_reads.rs}`, `test/net-harness/tests/{receipts.rs,saves.rs,scenario.rs,prototype.rs,fog.rs,auras.rs}`, `apps/verifier/tests/session_logs.rs`, `base/store` (`AppendWatch`), `test/rules`.
Goldens: none.

- [ ] `net-harness/tests/receipts.rs:45,78,132,162`, `saves.rs:77`: unbounded 1 ms sleep loops → `AppendWatch::wait_settled(timeout)` in `store`, which owns the writer thread.
- [ ] Per-file `hero` and `unit` helpers in the net-harness tests and `session_logs.rs` → `MatchUnits` on any `&World`.
- [ ] `test/rules`: 39 tests each walk 2,700 files and parse them again, and any edit reruns all → measure them in the junit of one run; if they are among the SLOW, one parse that the rules share.
- [ ] `package_set.rs` (223 lines): a second package loader that differs from the real one (`heights: None`, `max_move_speed: Num::MAX`) → one `Match::of_packages(&ModePackages, PackageStart)` above `package`, which replaces `PackageSet`, `Match::of_package` and `Arena`; the package tests move up with it. The largest move of the plan, last for that reason.
