# Workspace audit E: the open issues

Audit of every item of `.notes/ISSUES.md` (42 items) and `PLAN.md`'s Foundations (F1) at HEAD `68ca71cbc`, 2026-10-19. Read-only: no build, test or bench ran, so every timing item is unverified and marked so. Paths are relative to `source/` unless they start with `docs/` or `.notes/`.

Refactor parts used below: **boundaries**, the crates' boundaries; **dependencies**; **consolidation** of code Campfire and the stack both implement; **vocabulary**, one vocabulary; **integration**, the stack's integration (PLAN batch "Campfire on the stack" and its follow-up).

## Table

| ID | System | Title | Still true | Stack (`net-*`) | Root cause | Refactor reach | Size |
|---|---|---|---|---|---|---|---|
| E1 | Capabilities | `Action` names the order's sum and `actions`' action | yes | — | vocabulary | fixes (vocabulary) | M |
| E2 | Test packages | `packages/test/` sorted two ways | yes | — | vocabulary (layout) | fixes (vocabulary) | S |
| E3 | Workspace | `declare` a verb, `state-types` the one `-types` | yes | `net-despawn` breaks the same rule | vocabulary | fixes (vocabulary, boundaries) | S |
| E4 | Combat | client's `strike::<ClientForm>` matches no attacker | partly, likely stale | — | local bug / missing test | none | S |
| E5 | Import | rules package written from `format!` and mirror structs | yes | — | format (engine `*File` types are read-only) | touches (consolidation) | M |
| E6 | Mode | bounty paid for a site under construction | yes | — | local bug; needs a contract below `mode` and `production` | touches (boundaries) | M |
| E7 | Navigation | `route_units` scans every walker's change ticks | yes | — | architecture (no index fed by `Destination`'s writers) | none | M |
| E8 | Net-messages | serialize returns a `Result` no production sink can fail | yes | is the stack's own | format/API (inherited `io::Write` genericity) | fixes (consolidation, integration) | M |
| E9 | Net | LAN check flake on macOS after restart (stamp 300 applied at 301 and 303) | unverified (CI run) | the client's timeline comes from `net-sync` after batch "Campfire on the stack" | unknown (input timing after restart) | touches (integration) | M |
| E10 | Net | `Received a ping that is not present` flake | yes, in old net | gone: the stack has no ping; RTT from ACKs | upstream Lightyear | fixes (integration, batch "Campfire on the stack") | S |
| E11 | Net | client and local server share Bevy's global compute pool | yes | same: `net-stepper` and the emulator link keep two apps in one process | architecture (Bevy's process-global pools) | touches (integration) | L |
| E12 | Net | server's 2 ms frame runs every net system 500×/s | yes | partly: the stack's IO thread runs timers off the frame | architecture (frame pacing) | touches (integration) | M |
| E13 | Quic | 1 MiB send queue; Lightyear requeues reliable while queued | yes, in `quic` | fixed: resend only on loss; emulator models a rate; no join-burst scenario | upstream QUIC + missing test | fixes (integration); residual test S | S |
| E14 | Quic | no bench of the frame's systems, mutex waits, handshake flood | yes, in `quic` | partly: `admission` flood bench; no bench of the endpoint pass or full handshake | tooling gap | touches (integration) | M |
| E15 | Quic | allocations under the mutex (key update, `RecvBatch` arena) | yes, in `quic` | key update gone (ChaCha20 only); arena reuse pattern unchanged | upstream QUIC; arena design | fixes (integration), residual none | S |
| E16 | Release | `Actions::push` and kin skip `BookLoad`'s install check | yes | — | missing guard (test helpers) | none | S |
| E17 | Runner | 3v3 scripted match past 15 s | unverified | — | test cost | none | M |
| E18 | Runner | 3v3 resume test past 1 s | unverified | — | test cost (replay from 0) | none | M |
| E19 | Runner | drawn-values snapshot test past 1 s | unverified | — | test cost (sweep size) | none | S |
| E20 | Runner | flawed-snapshot test past 1 s | unverified | — | test cost (sweep size) | none | S |
| E21 | Script | list-of-names forms return untyped `ScriptList` | yes | — | format (script API types) | none | M |
| E22 | Stats | re-predicted unit keeps stale change ticks | yes (code); effect unverified | unknown: `net-prediction`'s switch inserts `Predicted` alone | architecture (change-tick passes vs hand-off) | touches (integration) | M |
| E23 | Verifier | two tests pass nextest's 5 s at times in a full run | unverified | swarm now takes every core (`threads-required`) | test contention / tooling | none | S |
| E24 | Areas | area `inside` and aura do one job, disagree on neutral and dead | yes | — | duplication of one rule | touches (consolidation, inside Campfire) | M |
| E25 | Client | drag threshold a constant; import skips `DragTolerance` | yes | — | format (import gap) | none | S |
| E26 | Client | slot charges read through progression's `Learning` | partly | — | crate boundary (charges are `actions`' rule) | fixes (boundaries) | S |
| E27 | Combat | splash `<` radius vs reach rule `≤` | yes | — | local bug (second reach rule) | none | S |
| E28 | Mode | spatial map loads height grid and ground the sim never reads | yes | — | format (load accepts what the sim ignores) | none | S |
| E29 | Net-common | two bench sampling policies (`Scale` vs per-group `sample_size`) | yes | is the stack's own | duplication | fixes (consolidation) | S |
| E30 | Net-interpolation | no test that an unchanged sample writes nothing | **no — fixed** | test exists | — | — (delete item) | — |
| E31 | Net-stack | three counting allocators | yes | one of the three | duplication | fixes (consolidation) | M |
| E32 | Net-stack | 13 Bevy hooks named `on_*_hook` | yes | is the stack's own | vocabulary | fixes (vocabulary) | S |
| E33 | Net-stack | no drive lane: lint without `compression_lz4`, rustdoc `-D`, coverage | yes | is the stack's plan | tooling gap (config + one new lane) | touches (integration, dependencies) | S/M |
| E34 | Net | fragment count sizes an allocation before any check | yes, in the fork used today | fixed in `net-channels` | upstream Lightyear | fixes (integration, batch "Campfire on the stack") | S |
| E35 | Net | an unjoined link uses every channel with no budget | yes | stack knows no "joined"; Campfire must gate it | architecture (admission above the transport) | touches (integration) | M |
| E36 | Net | offer, input ack, chain history allocate per message | partly | stack queues typed values; `InputAck`'s `Vec` stays Campfire's | local (message shapes) | touches (integration) | S |
| E37 | Net | `SimClient` keeps the session `Keypair` (Copy) for the app's life | yes | — | architecture (`Plugin::build(&self)`) | touches (integration) | S |
| E38 | Orders | `PlayerOrders` a scratch with a systems' `<X>Orders` name | yes | — | vocabulary | fixes (vocabulary) | S |
| E39 | Quic | `admit` walks every peer per new connection | yes, in `quic` | fixed: per-address counts map | local, in deleted code | fixes (integration, batch "Campfire on the stack") | S |
| E40 | Store | `SourceFile::production` ends a gated item inside a raw string | yes | — | local bug in a hand-made parser; parser lives in the file proxy | touches (boundaries, consolidation) | M |
| E41 | Verifier | nothing verifies a segment from its checkpoint's `LogCarry` | yes | — | architecture (planned feature) | none | L |
| E42 | Workspace | `clippy.toml` head says `common`'s tests hold the bans | yes | — | doc drift | touches (dependencies, integration: batch "Campfire on the stack" edits `clippy.toml`) | S |
| F1 | Quic | no load scenario for the QUIC thread | yes, for `quic` | partly: emulator has `RateLimit`; admission flood bench; no endpoint-pass load bench | tooling gap | fixes the QUIC part (integration); residual re-homes | M |

## Evidence

### Decide

- **E1.** `capabilities/capabilities/src/order/mod.rs:23` `enum Action { Orders(OrdersAction), Progression(ProgressionAction), Production(ProductionAction), Items(ItemsAction) }`; the other `Action` is `capabilities/core/actions` (`ActionBook`, `action_book.rs:18`; `Actions`, `lib.rs:169`). The bots' order scripts hold `Action<Named>` (`session/bots/src/order_script/mod.rs:54`). Still a decision for the user; the refactor's vocabulary pass is where it lands.
- **E2.** `packages/test/{heroes,modes,rules}` by what, `packages/test/moba/{heroes,modes,spells,locales}` by game. Unchanged.
- **E3.** `crates/capabilities/core/declare`, `crates/capabilities/core/state-types`. The stack adds `net-despawn`, a verb, against the same noun rule.

### Research

- **E4.** `combat/src/attacks.rs:401-426` still takes `&mut ClipShots`. But `ClipShots` is `Replication { KIND: Cooldowns, Sending: SentPredicted }` (`combat/src/clip_shots.rs:82-85`): the owner's client receives and predicts it. `server_part::<ActionSlots, ClipShots>()` (`combat/src/lib.rs:172`) only makes it required on the server. So an owned predicted attacker should match once the server's copy arrives; the gap is any unit before that copy, and the test inserts it by hand (`capabilities/tests/combat/mod.rs:237`). Not checked in a running client.
- **E5.** `apps/import/src/zero_hour/rules_package.rs:37` `mode_manifest`, `:79` `mode_data`, both `format!`; `unit_types.rs:72` `UnitsFile` a `Serialize` mirror. Cause: the engine's file types are deserialize-only, so the import cannot write through them.
- **E6.** `mode/src/bounty_book.rs:114-160` checks unit type, killer's owner, team and tags, nothing of construction. Construction state lives in `production/src/construction/`; `mode` would need it through a contract below both (AGENTS.md Direction).
- **E7.** `navigation/src/routing.rs:45` `Ref<Destination>`, `:65` `destination.is_changed() || route.is_changed()`. `Destination` (core `units`) is written in `orders`, `production`, `abilities` (`casts.rs`) and `units`.
- **E8.** `net-messages/src/serialize_fns.rs:123` `SerializeFn -> Result`; `:166-176` the default serialize `expect(unnecessary_wraps)` and always `Ok`. `net-messages/src/send_queue.rs:151-157` drops on `Err`. Answer to the open question: every production sink is infallible — `net-bytes/src/writer/mod.rs:33-42` (`Writer::write` returns `Ok`) and `net-channels/src/packet_builder/mod.rs:30-39` (`PayloadWriter`); a `ToBytes` write fails only because `ToBytes::to_bytes` is generic over any `io::Write` (`net-bytes/src/to_bytes.rs:21`, `writer/mod.rs:143`). `varint::len` only `debug_assert`s its 2⁶²−1 bound (`net-bytes/src/varint/mod.rs:8-9`).
- **E9.** Not reproducible here. The verdict is `checks/lan-check/src/verdict/`; inputs land by Campfire's own placement. After batch "Campfire on the stack" the client's input timeline is `net-sync`'s (`InputTimelineConfig`, `net-sync/src/lib.rs`), so the item should be re-run against the stack before research.
- **E10.** Old: `lightyear_sync::ping::manager`. The stack has no ping manager; `docs/design/13-net-stack/transport.md:79-83` (An idle transport sends its tick: RTT samples from ACKs, "Refused: ping messages"). PLAN batch "Campfire on the stack" already takes it.
- **E11.** `net/src/sim_server/sim_server_plugins.rs:28` `TaskPoolPlugin::default()`; same in `apps/client/src/client_module.rs:148`. Bevy's pools are process-global. The stack keeps two apps per process: `net-stepper/src/stepper/mod.rs:116,272`, and batch "Campfire on the stack"'s local link is the emulator in-process.
- **E12.** `net/src/net_protocol/mod.rs:75` `FRAME = 2 ms`; `apps/server/src/main.rs:196` `run_loop(NetProtocol::FRAME)`. The stack's IO thread runs the endpoint pass when a datagram or timer is due (`docs/design/13-net-stack/session.md:35-41`), so the frame no longer has to run for timers; the 2 ms loop is Campfire's own and outlives batch "Campfire on the stack" unless changed.
- **E13.** QUIC part dies with `quic`. Stack: messages wait in the channel's `Vec` until the session budget (`transport.md:50-57`), a reliable resend only after the session finds the packet lost (`transport.md:58-61`); `net-emulator/src/rate_limit.rs:1-20` models speed and queue. Residual: no scenario measures a join burst's delay.
- **E14.** Stack has `net-udp/src/admission/bench.rs` (flood and rejoins of `Admission::admit` alone). No bench of the endpoint pass under its mutex, of a handshake flood through `secure_establishment`, and no bound of new connections per pass (no match for such a limit in `net-udp`, `net-packets`).
- **E15.** Key updates: `net-udp/src/tls_provider.rs:7` ChaCha20-Poly1305 only, "no key update". Arena: `net-udp/src/recv_batch.rs` copies into `net-common/src/arena/mod.rs` chunks, which allocate a new chunk while views are held past `max_spares` (`arena/mod.rs:6-8`): same pattern, by design.
- **E16.** `capabilities/core/declare/src/book.rs:49-62` `BookLoad::load_into` asserts no `SimUpdate` schedule; `capabilities/core/actions/src/lib.rs:411-416` `Actions::push` does not; `units/src/units.rs:445` `give_type`, `stats/src/lib.rs:665` `give_type_stats` likewise.
- **E17–E20.** Tests at `test/harness/tests/runner/match_3v3/mod.rs:297`, `runner/resume.rs:148`, `runner/proving_match.rs:430` and `:384`. `.config/nextest.toml` gives 1 s (5× to terminate) and 15 s ×4 for `scripted_match`. Timings **not checked** (no test run allowed). Bevy 0.20 in batch "Campfire on the stack" may move them.
- **E21.** `production/src/production_api.rs:77-83` `queue` → `ProductionColumn::queue` → `Checked<ScriptList>` (`production_column.rs:189`); `mode/src/mode_api.rs:483` `paths` "the paths' names". `NameKind` exists (`core/values/src/name_kind.rs:8`) but these forms carry none.
- **E22.** `net/src/sim_client/mod.rs:764-788` `mark_unpredicted` observers. Stack side not settled: `net-prediction/src/switch/mod.rs:182` defers an insert of `Predicted`; whether it rewrites the parts is **not checked**.
- **E23.** `session/verifier/tests/binary.rs:22`, `test/harness/tests/verifier.rs:222`. Not run. The issue cites `drive.py`, which is gone; the net-stack swarm now takes every core (`.config/nextest.toml`, `threads-required = "num-cpus"`).

### Ready

- **E24.** `areas/src/area_holds.rs:46-58` (`targets.living_at`, `Hostile | Neutral => enemies`), `areas/src/area_spec.rs:41-51`; `stats/src/held_pass.rs:136-149` (`filter.selects(relation, tags)`). Line numbers in the item are stale.
- **E25.** `apps/client/src/press.rs:8` `DRAG_PIXELS = 4.0`; `apps/import/src/zero_hour/camera_import.rs:36,123` reads `DragToleranceMS` only.
- **E26.** `progression/src/learning.rs:12-15` `Learning` holds `ActionBook` and `SlotKindBook`, both `actions`' resources; `:46-48` `charges`. `apps/client/src/hud/mod.rs:27,104-108` reads charges through it. Whether a mode without progression shows none is **not checked** (the resources are `actions`'); the misplaced owner is real either way.
- **E27.** `combat/src/attacks.rs:131` `volume.nearest(point, splash.radius).is_lt()`; reach rule `geometry/src/metric.rs:51` `reaches`.
- **E28.** `capabilities/core/units/src/map_book.rs:44-53` `Metric::Spatial => position`; `:67-70` `highest_along` none on spatial.
- **E29.** `net-common/src/bench/scale.rs`; `test/harness/src/bench/{client_frame.rs:61, server_tick.rs:43,99,127, match_build.rs:27,40,58,78, server_frame.rs:42}` `sample_size(..)`. The stack's own decision: `docs/design/13-net-stack/testing.md:86` (one sampling policy).
- **E30.** Fixed: `net-interpolation/src/interpolate/tests.rs:335-357` `an_unchanged_sample_is_not_a_change` asserts the change tick holds over a second update at the same interpolation tick. Delete the item.
- **E31.** `base/common/src/alloc_audit/counting_allocator.rs` (process window, all threads but main), `test/harness/src/bench/allocations.rs` (process-global count), `net-stack/tests/alloc/allocator/mod.rs` (per-thread). Three designs; the stack's is argued in `testing.md:72-77`.
- **E32.** 13 functions: `net-messages/src/{event_sender.rs:39,48, message_sender.rs:59,68, host_message_sender.rs:49,58, message_receiver/mod.rs:111,120,273,282, host_event_sender.rs:49,58}`, `net-link/src/server.rs:100`. Six of them are the same add/remove pair on six sender/receiver types.
- **E33.** `docs/design/13-net-stack/testing.md:115-123` plans both. The driver already supports several lint lanes and a rustdoc lane (`drive/src/lanes.rs:18-27`, `lint: Vec<FlagSet>`, `docs: bool`), and `source/Cargo.toml`'s drive table sets neither: the first two are configuration; coverage needs a new lane.
- **E34.** Fork `54e6be2`, `crates/transport/transport/src/channel/fragments/receive.rs:136-147` (`vec![0; num_fragments * fragment_size]`), patched in at `source/Cargo.toml:477-495`. Stack: `net-channels/src/fragment_receiver/mod.rs:126-155` refuses a count above `min(max_message_size, MAX_FRAGMENTS)` before any allocation.
- **E35.** `net/src/net_protocol/mod.rs:81-117` channels with directions only; `net/src/sim_server/offering.rs:41` `ANSWER_WINDOW`, `:180-196` `close_silent`, `ToAnswer`. No per-link budget before `Joined`. The stack has a send budget per session, not a receive gate on "joined", which is Campfire's notion.
- **E36.** `net/src/net_protocol/input_ack.rs:17-27` collects a `Vec` per ack (true). `offering.rs:260-264` sends an `Arc::clone` (no allocation in Campfire's code; the old cost was Lightyear's send). `sim_client/chain_history/mod.rs:13,33` is a `VecDeque` pushed per input, amortized, not per message: partly stale.
- **E37.** `net/src/sim_client/mod.rs:98-102` `pub session_key: Keypair` on the `SimClient` plugin.
- **E38.** `orders/src/unit_orders.rs:24` `PlayerOrders(Vec<(Entity, UnitOrder)>)` beside systems `items/src/trade_orders.rs:15` `TradeOrders`, `progression/src/learn_orders.rs:14` `LearnOrders`.
- **E39.** `network/quic/src/endpoint_state.rs:204-211` walks `peers` per admit. Stack: `net-udp/src/secure_establishment/server_side.rs:127-145` reads a `counts: BTreeMap<AddressKey, _>` kept on hold.
- **E40.** `base/store/src/source_files.rs:139-166` `item_end`: `depth < indent` and the `;`/`}` rule fire inside a raw string. `rules` reads sources through it (`test/rules/src/{cycles,world,workspace,citations,errors,manifests}.rs`), while `checks/vocabulary` parses Rust with `syn` (`checks/vocabulary/Cargo.toml:14-16`). Two Rust readers, and the line-based one lives in the file-I/O proxy.
- **E41.** `session/protocol/src/checkpoint/log_carry.rs`; the log writes the carry and reads it back only to resume (`session_log/mod.rs:984-1011`, `:1142`); `session/verifier/src` names no `LogCarry`.
- **E42.** `source/clippy.toml:6-11` "`campfire-common`'s tests hold each group and each exemption".
- **F1.** `PLAN.md` Foundations F1; batch "Campfire on the stack" deletes it with `quic`. What remains for the stack is E14's residual and E13's join-burst scenario.

## Groups by root cause

1. **The old network stack, replaced in batch "Campfire on the stack"** — E10, E13, E15, E34, E39, F1 (and the QUIC half of E14). Lightyear 0.30 (the fork) and `quinn-proto` carry the defect; the in-tree stack already fixes each at the cited lines. They close when batch "Campfire on the stack"'s checks pass; only E13's join-burst scenario and E14's endpoint-pass bench survive, re-homed as stack test gaps.
2. **Campfire's glue over Lightyear's app model** — E9, E11, E12, E22, E35, E36, E37. The `net` crate is shaped around Lightyear's plugins (frame loop, plugin-held secrets, observers on `Predicted`, admission by channel direction). Batch "Campfire on the stack" rewrites this glue, so each is cheapest to fix while it is open; E11 survives any port (Bevy's global pools).
3. **The stack copied in as a second world** — E8, E29, E31, E32, E33. The stack arrived with its own conventions and tools: its bench policy, its allocator, its `ToBytes` contract beside `codec`, Bevy's hook names, its planned lanes. One workspace should have one of each.
4. **Names that break "one word, one meaning"** — E1, E2, E3, E32, E38. Each author's names, never reconciled; the stack adds `net-despawn` and the `on_*_hook` names.
5. **Ownership in the wrong crate** — E26, E40, E6, E5. A rule held by a crate that is not its owner: charges in `progression`, a Rust source parser in `store`, construction status unreachable from `mode`, and the engine's file sections unwritable, so `import` mirrors them.
6. **One rule, two implementations, in capabilities** — E24, E27. A reach or a selection coded twice, which drift (`<` vs `≤`, neutral and dead units).
7. **Passes that find changes by scanning change ticks** — E7, E22. No writer-fed index, so passes scan, and a hand-off that keeps old ticks hides a unit.
8. **Test cost and contention** — E17, E18, E19, E20, E23. Replay-from-zero and sweep sizes; all unverified here.
9. **Format and design gaps, local** — E4, E16, E21, E25, E28, E41, E42. Each standalone.
10. **Stale** — E30 (fixed), E4 (likely stale).

## Fold into the refactor

- **E8, E29, E31, E32** (consolidation and vocabulary of the stack): they are the "two implementations of one thing" and "one vocabulary" work the refactor is about. E8 decides whether `ToBytes` stays beside `codec` or becomes infallible over Campfire's own sinks; E29/E31 pick one bench policy and one allocator for the workspace; E32 renames the hooks and merges the six duplicate add/remove pairs.
- **E1, E3, E38** (vocabulary): renames across crates; E3 applies to the new `net-*` names in the same pass (`net-despawn`). E1 needs the user's choice first.
- **E26, E40** (crate boundaries): `Learning::charges` moves to `actions` and the client drops a `progression` dependency for it; the source scanner leaves `store`, and `rules` reads Rust with the `syn` parser `vocabulary` already uses, which fixes E40's bug by construction.
- **E33, E42** (tooling with the stack as first-class): two drive table lines for the lanes the driver already supports; the `clippy.toml` comment fixed when batch "Campfire on the stack" edits that file. Decide whether `compression_lz4` is needed at all (dependencies).
- **E10, E13, E15, E34, E39, F1** (stack integration): already scheduled; batch "Campfire on the stack" should delete them once its checks pass, and file the two residual test gaps (E13 burst, E14 endpoint pass) as stack items.
- **E12, E35, E37, E36** (glue rewritten in batch "Campfire on the stack"): fix while the `net` glue is being rewritten — the frame loop paced by tick and IO, a receive gate before `Joined`, the session key taken out of the plugin, `InputAck` without a per-ack `Vec`.
- **E30**: delete from the log now.

Not reached by the refactor: E4, E6 (needs a contract, but is a rules fix), E7, E16–E21, E23–E25, E27, E28, E41.
