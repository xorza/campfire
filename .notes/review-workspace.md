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

**Target:** the verdict carries what the fix needs. In the driver: a count and the first error in the verdict, and every deduplicated `file:line: message [lint]` in `last-<command>.txt`; each failed test's panic location and first lines; a command's error pattern from its table; a TIMEOUT of its own; the build of the test binaries as a step of its own; the full run's SLOW lines kept with their names. In this repository: `checks/vocabulary` goes, and the commands table gains `client` and `server`. The driver's items belong to the `drive` repository; 0.2.4 holds the error count and list, each failed test's output and the SLOW lines, and the other three wait for the driver's own work.

### R4. A sim mechanism stops short of its plumbing

- **Timed processes.** `TimedProcess::due` states when a unit is due, but for each process kept `BySetters`, the arrival of its part is a hand-written hook (nine: `areas/src/area.rs:51`, `combat/src/respawn.rs:23`, `navigation/src/route.rs:104`, `production/src/kept_spawns.rs:240`, `production/src/train_queue.rs:94`, `production/src/order_dues.rs:75`, `orders/src/next_turn.rs:76`, `stats/src/modifiers/modifier_clocks.rs:59`, `stats/src/modifiers/mod.rs:102`), and each setter goes through a hand-written handle of the same shape (five: `RouteDues`, `GoalDues`, `OrderDues`, `QueueDues`, `ModifierDues`).
  **Target:** `TimedProcess` gains `type Arrives: Component`, the part whose insert gives a unit its time, with no default; `DueIndex::track` installs one generic insert observer beside its `predicted` one, which reads `P::Parts` and `P::Books` and pushes `P::due`. One `UnitDues<'_, P>` replaces the five handles.
- **Named ids.** The match's and the players' entities are found through `EntityIndex`'s B-tree on every call (`OnMatch::get`, `PlayerRows::of_player_slot`, `UnitsWorld::relations`), per candidate in `orders/src/acquiring.rs:113-116` and per row in `units/src/row_source.rs:108`. PERF.md measured `OnMatch<Relations>::get` at 10.5 % of `gather/two_fields`, and fixed it only inside the gather loop. The named ids are a fixed set: `StableId::MATCH` and one per player slot after it.
  **Target:** `EntityIndex` keeps the named ids' entities in an array by name beside the B-tree of allocated ids, so a named lookup is O(1) for every caller with no API change; its iteration stays in id order, as every named id is past every allocated one.
- **The gather loop reads its own writes as changes.** `GatherLoop::run` takes `Changed<Gatherer>` through a `SystemState` whose last run is before its own `Gatherer::set_in` writes, so the next tick sees them and `free_left_nodes`, a scan of every node with a sort, runs after any worker stepped, against its doc.
  **Target:** a worker that leaves its node says so where it leaves: `Gatherer::set`, which every writer calls, writes a `LeftNode` message, and `free_left_nodes` reads the messages and `Removals<Gatherer>`, with no change tick to infer from.

### R5. I/O and work traded for memory, and cold paths that run serially

- `Session` holds its role as two `Option`s, `signer` and `journal`, and `Self::server(...).expect` panics when a verifier's session is asked to serve.
- `PackageStore::scan` decodes each index and drops it; `find` reads and decodes it again.
- The Zero Hour import (15.0 s of a 27.3 s check run, `docs/research/debug-loop.md`) runs on one thread, although its output order is fixed by `BTreeMap` keys; `threads::Parallel` exists only under `internals`. The install's 32 archives are hashed in 1 MiB chunks, each read into a new zeroed `Vec` with an `fstat`.
- `DurableDir::create_all` asks `Path::is_dir`, which `clippy.toml` bans and 02-engine-core.md, Bounded reads, no existence checks, refuses; `PackageWriter::write` calls it, with a directory sync, per file.

**Target:** `enum SessionRole { Server { signer, journal: Option<Journal> }, Verifier }`. `StoredPackage` keeps its `Arc<FileIndex>`. `threads` gains an ordered parallel map with a bounded window in production, and the import hashes the install's archives through it; an unchanged import is kept, not written again. `store` gains `InputRanges::read_at_into`. `create_all` follows std's `create_dir_all`: create, and on `NotFound` create the parent first.

### R6. The packet layer assumes about one packet per frame each way, and prediction scans everything

An ACK names the largest number and the 32 below it, packets are accepted once per frame, and more than 64 in a frame are dropped. When a burst puts more than 33 packets between two of the receiver's frames, the oldest are never acknowledged, so they are declared lost, sent again, and the congestion window halves. The one-byte ACK delay saturates at 63.75 ms, so a slow frame inflates the RTT sample that sets input delay. On the client, the rollback scan visits every predicted entity for each component type on every confirmed tick, and writes an anchor into each history. Both are claims from reading the code, with no case that shows their cost.

**Target:** measure first, with a net-emulator case of a burst past 33 packets in one receiver frame and a bench of the rollback scan over idle units. Then one window of 64 for the ACK bits, the replay window and the packets a frame accepts, a single constant, so an ACK names every packet a frame accepted; and an ACK-only packet at the end of a frame whose unacknowledged packets pass half the window, as QUIC acknowledges every second packet (RFC 9000 §13.2.2; quinn's `ack_eliciting_threshold` of 1). RFC 4303 §3.4.3 prefers a window of 64 and lets a receiver take a larger one. Then an ACK delay that cannot saturate silently, and a scan over the pairs that gained an entry.

### Decisions taken

Taken 2026-10-24, after a look at the field's practice:

- **Radius scans stay apart.** `UnitGrid::visit_near` is the shared walk, as Unity's `OverlapSphere` and Avian's `SpatialQuery` are, and each scan keeps its own filter; a `Targets::within` of two closures enforces no rule the reach rule's own functions do not.
- **A restore checks every signature.** The journal's unkeyed BLAKE3 frame hash finds a torn write, not a changed record; PostgreSQL and Raft trust their log only because their threat model gives the disk to the server alone, and systemd's keyed sealing shows the cost of a seal of one's own (IACR ePrint 2023/867).
- **Publish keeps two buffered walks,** as `qt-faststart` writes a header-first file from a log; a live shape costs 4 bytes a tick and a path for a restored server, for a walk once a session. An abort, rarer still, keeps its read too.
- **A checkpoint's record keeps its second encoding,** so a signature covers one canonical message, not pieces whose boundary the format must make unambiguous.
- **The import is kept by a hand-raised version.** An unchanged install and importer version reuse the last import, as Unity keys an import on its importer's version and Unreal its DDC on a version string; a change to the importer's output raises the version.

## Plan

### Rules for every batch

- **A bug's test fails first.** Each bug item names the test that fails before its fix and passes after it.
- **A performance item is measured.** It names its bench case. The change stays only when the median of interleaved pairs moves past the case's noise floor, by the method of `.notes/PERF.md`. A case that does not exist is added first, in its own commit.
- **A design decision that changes is written in the same batch,** in its system's document, with the source the item names, and its citations in code follow.
- **Each batch says whether goldens move.** A golden that moves names why; one that moves in a batch that says none is a defect of the batch.
- Each batch ends with `cargo drive gate` green and a stop for review. Whoever addresses an item deletes it.

### Batch 4: the driver keeps its signals (R3)

These items change the `drive` repository and wait for the driver's own work; then the drive table requires the release that holds them.

- [ ] Driver: a command's first error is its log's first line → an `error` pattern per command in its table; the Zero Hour check's is `ERROR|panicked at|error\[`.
- [ ] Driver: a timeout reads as a FAIL → a TIMEOUT of its own in the verdict.
- [ ] Driver: a compile error of the test binaries shows as `FAIL index`, as the `index` step runs `test(=index)` → the build is a step of its own.

### Batch 7: the passes that a measurement shows

A profile of `server_tick/mean_3v3` (perf, 2 kHz, one run) showed none of `IncomeClock::pay`,
`Victory::follow`, `Dying::despawn_dead` and `RoomMaking::make_room` over 0.3 % of samples, the
nested `resource_scope` of `ProgressDues::with` at 0.42 % and `DueIndex`'s tree at 0.31 %, so their
items left the plan by its rule. Left:

- [ ] `capabilities/tests/alloc`: the battle's budget names one allocation, Bevy's: `BundleRemover::remove` (`bevy_ecs` 0.20, `bundle/remove.rs:172,193`) collects the removed components into a `Vec` whenever the archetype has a `Discard` or a `Remove` observer, since the filter of bevyengine/bevy#17942 → a fix in Bevy, which passes the filtered components as the despawn path passes its archetype's, proposed upstream; then the release that holds it, and the budget goes to zero. Decided 2026-10-24: not hooks on each of the 86 state components, which a derive attribute on each would hold.
