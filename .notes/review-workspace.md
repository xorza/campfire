# Review: the whole workspace

Scope: every crate under `source/`, except the test harness. Paths are relative to `source/`. Line numbers point to commit `22695ff7`.

## Proposal

The crate graph holds its layers, and a test holds them (design 02, The capabilities' crates). No module cycle crosses a layer: the only cycles are in `log` (a test reach-in) and `zero-hour-check` (its pins). The defects are inside the crates. Eleven causes explain most of them.

### 1. A rule the protocol states is not a rule the log holds

The verifier proves less than design 05's "A check proves" says. `check_result` accepts `aborted` for any match. A delegation has many spellings, so one log has many fingerprints. Nothing refuses one main key in two slots, or a rejoin that restarts a chain from the same root. The file bound counts payload bytes only, so a long session writes a journal that its own reader refuses. `LogCarry` is written and never read, so "a segment verifies from its checkpoint alone" is false.

Target: each rule of design 05 is a check in `SessionLog` or in `check_result`, or the design marks it planned. Checked values are types: a `Delegation` is canonical when parsed, and `ServerSeeds` is checked when built.

### 2. "Zero life is dead" holds by convention only

A max-life rise lifts a dead unit's life above 0, so an area kills the corpse again. `ctx.kill` sets life to 0 and the unit dies at the next tick's deaths, as its API says, but that tick's regen runs first and the unit lives. Damage queued after the pass sits in a scratch past the tick's end, which a snapshot drops.

Target: one living test (not `Dead` and life above 0) that the pass, regen, the refresh and the attacks read. A life at 0 stays at 0 until the death or the respawn. A scratch refuses a push outside the window that drains it.

### 3. A restore check covers less than the load and the systems take

Design 02 says a restore gives an error, never a panic. These restores pass and then panic or diverge: a box body on a spatial map, `Points` past the level, a user without its use slot, an `ActionStart` charge outside 0 to 1, a unit with a clipped weapon and no `ClipShots`. Navigation's `StaticChanges` is the diff of a cache against its last fill, so a restored match re-asks routes the original did not.

Target: each fact the load refuses, and each relation a system `expect`s, is a check of the part that holds it. No state that a restore cannot rebuild outlives the run that made it: a fresh cache's first fill is no change.

### 4. Rounded intermediates feed a second rounding

`Vec3::normalized`, `SinCos::of_degrees`, `Direction::length`, `rotated_deg` and `Bounds::exit` each claim one rounding and chain two or three. Their tests compare against the same chain. The box tests take an unsigned distance where the circle tests take a signed one, so a walker of radius 0 walks through every building.

Target: every operation derives from math's one wide primitive and rounds once, each doc states its bound in bits, and each differential test compares against an exact reference. "Closer than r" means "enters the open shape grown by r" for every shape.

### 5. A module's additions are not one walk of one list

The mode is outside `Release::modules`: `declare` and the install walk two lists, and `MatchEnd::stop_stages` goes into the schedule by hand. `Units::open` inserts five non-sends by hand, because `Declarations::non_send` cannot say "only in some roles", and the resource audit cannot see non-sends. `Book::OWNER` restates the declaring module. The kinds the release runs are written three times. `hold_charges` installs on `Combat`, where it needs nothing of combat.

Target: one list of modules, the mode included, walked once for declare, install, audit and binding. `non_send` returns an `Option`. The owner of a book is the module that declares it. Each install gate names the capability whose resources the system reads.

### 6. The registry is not the one source of what a script load checks

`ScriptFacts` sees only method-style calls, so `damage(ctx, …)` passes the load, and the release binds every capability's API into every match. Times and player slots have no `ScriptArg` type, so each capability resolves them by hand with three return shapes. `DATA_ACCESSORS`, Rhai's `get$` spelling and the hook arity are kept by hand beside the registry. There are two call flows: `hook_call` and a hand-made begin, call and apply in mode and orders.

Target: `ApiBuilder` records everything the load reads. Every argument that needs a check is a `ScriptArg`. The match binds only its declared capabilities. One call entry returns a `CallError` with a "pool spent" case.

### 7. A pass visits everything, not what changed

The Direction says "a pass visits what changed". `ActionSlots` holds the slot table and the per-action progress, so every attack and channel step re-triggers the change-driven hold passes and replication. `DueRun` does not deduplicate, so the heap grows each tick. Many passes scan every unit each tick: dying, held modifiers, attacks (three times), nodes, supply, AI thinking, the shop, placement, `keep_in_bounds`. In the client, the panels, the HUD and the bars rebuild each frame.

Target: the progress is its own component. One due-run scheme with a dedup. Each pass takes `Changed`, `Added` or an index.

### 8. An entity does not spawn whole

A unit moves between archetypes up to eight times, as `KindSpawn` reads the core back from the entity. Also: a player's `Seated`, a timed spawn's `Lifespan`, `Rally`, a grant's `TimedSlots`, a QUIC link's `RemoteId` and `Linked`.

Target: `KindSpawn` takes the core values, and each spawn is one bundle. A part every producer carries is declared and written in place.

### 9. Names and paths cross the boundary as strings

Bot orders look up unit types by name each time an order is due. The importer and the client build package paths with `format!`, then parse them with `expect`. The client-file types return `String` paths, and some files have no path function. A dependency path is a raw string joined as an OS path. Damage types are `&str` plus indices.

Target: each name resolves once, at the load or the start. Each format type owns its `PackagePath`. Each name of the original becomes an engine name or a path through one checked function.

### 10. Sibling types grew one at a time

The three store writers report failures in three shapes, and two of them lose the last failure at drop. `DurableFile::create` and `DurableDir::create` mean opposite things. Areas and projectiles differ in each role. Math has six mul and div entry points with no pattern. Nine INI readers copy one block grammar. The two client panels copy one widget.

Target: one contract for each family, and the siblings implement it.

### 11. Two paths build one thing

The net server builds a match on start and on restore with parameter bags, and seats a player twice and attaches the journal twice. `SessionLog` has 27 fields and seven jobs, and a rewound log is a hidden mode. The importer threads one mutable `ImportNames` through every stage, so the stage order is a hidden contract. `ModeSpec` copies package facts, and the spec is reached three ways.

Target: one `MatchRun` that both paths produce. `SessionLog` composes an entry store, a slot controller, an input schedule and segments. The importer builds the closed set of unit types first, and the stages return their additions.

### What the decisions follow

Each decision this plan holds, and the practice it follows.

- **A joined chain's root from the `Join`'s place.** A sequence that restarts at a session boundary lets old messages be replayed; the usual fix is a value unique to each session, given at resumption, as TLS's resumption nonces are.
- **Damage after the pass kept as state.** A restore or a rollback needs the whole state, pending events included (GGPO's save and load contract).
- **One living test; regen does not lift 0.** A unit at 0 life that still takes heals or targets is a known class of bug (Spring's dying units stay targets); the fix is one place that decides death, which every other system reads.
- **A spent pool fails an action's hook.** Warcraft III ends a thread past its operation limit, StarCraft II stops a thread past its budget with a trigger error, and Roblox ends a script that does not yield. None defers the call.
- **`on_interval` effects planned.** Unreal's Gameplay Ability System runs periodic effects from data alone, and Dota 2 from script (`OnIntervalThink`). A data-only import, as Zero Hour's toxins, needs the data form; the slice that imports them builds it.
- **Floats only in the importer.** Photon Quantum converts floats only in the editor, never in the simulation, and marks its float conversions unsafe.
- **Tint the look.** Zero Hour, Age of Empires and Timberborn tint the ghost; StarCraft II also colors each cell of its footprint grid, and Zero Hour marks the body that blocks.
- **Refuse unseen ground.** Zero Hour refuses a build point the player does not see, so a hidden body never causes a refusal.
- **`overflow-checks` kept.** Its cost is often small but can stop vectorization; an optimization session measures it.

## Plan

Each batch is one commit and ends with `./drive.py check` passing. Whoever addresses an item deletes it.

- **Tests.** Each bug item lands with a test that fails before its fix, built on the item's failure scenario. Each changed behavior is tested; a pure refactor is not.
- **Designs.** Where an item's code and its design disagree, the batch changes the one the item names, in the same commit.
- **Goldens and formats.** A batch marked "moves goldens" blesses them and says why each moved. A batch marked "format" raises the version tag of each format it changes. Any other batch moves no golden.
- **Order.** By impact: what lets a wrong log verify, then what makes a match diverge or panic, then what breaks a rule of the Direction, then the cleanups. A batch that needs another names it.

### 9. Mode and production

`mode`: `mode_effect`, `mode_call`, `mode_api`, `mode_spec`, `timers`, `loadout_setup`, `roster`, `mode_schema`, `mode_input`, `mode_hooks`, `mode_map`; `production`: `gather_loop`, `placement`, `production_orders`, `trains`, `player_supply`, `production_column`, `construction`, `gatherer`, `production_api`; `combat`: `dying`.

- [ ] production/src/placement.rs:51-57, 263-306; docs/design/11-rts-foundations.md D8, docs/design/04-capabilities/production.md:49 — a static body blocks a placement whether the builder's team sees it or not, so a build into fog reveals a hidden enemy building. Zero Hour (TheSuperHackers' `BuildAssistant::isLocationLegalToBuild`) refuses a build point the player does not see, and reports a fogged blocker as shroud, "so that players cannot use this info to determine whether they exist". → A placement passes only where the builder's team sees every cell under its box; a hidden body is then never a refusal's cause. D8 and production.md change; the client's preview refuses unseen ground, from the same test. **Skipped: see `review-workspace_QUESTIONS.md`, batch 9, item 5.**

### 10. Combat and stats: one modifier application

`stats`: `modifiers`, `grant_pass`, `held_pass`, `type_passive`, `modifier_book`, `application`, `modifier_params`, `refresh`; `combat`: `attacks`, `combat_hooks`, `combat_event`, `recent_attackers`, `damage_pass`, `combat_book`, `modifier_intervals`, `mode_combat_data`; `capabilities`: `book_builder`; `actions`: `slot_times`; `package`: `load_check`.

- [ ] combat/src/attacks.rs:188-217, 272-277; actions/src/action_slots/slot_times.rs:313-336 — `pay_attack_toggles` pays the toggles before `strike` tests the weapon's own cost. When the rest is too little, the windup ends, and the toggle cost is paid again each windup. → Pay the toggles only for attacks that afford both together.
- [ ] stats/src/modifiers/mod.rs:131-132, grant_pass.rs:208-227, held_pass.rs:156-162, type_passive.rs:44-59, modifier_book.rs:254, modifier_params.rs:44-54 — four near-copies apply a modifier. With `reapply = "ignore"`, `apply` returns before it joins the new hold, so a toggle's held modifier ends at its own duration and is applied again each pass; `HeldPass` calls `carried.hold` instead. Every caller passes the same param closure, and `TypePassive`'s `Release` arm is dead. → One operation on `CarriedMut` that grants or applies (id, applier, `&ParamBook`, source) and joins a hold on an existing instance whatever `reapply` says, used by `Stats::add_modifier`, `GrantPass`, `HeldPass` and `TypePassive`.
- [ ] stats/src/held_pass.rs:105-207 — pushes every player modifier for every living unit each tick, sorts, and scans every instance for aura carriers. → Player modifiers as a grant source of changed units; an index of aura carriers.
- [ ] combat/src/attacks.rs:155-336 — three systems each scan every living unit and compute `going_off`, `wielded` and `strikes`; `attack_events` is exclusive and runs each tick. → Collect the tick's going-off attackers once.
- [ ] stats/src/application.rs:25-39, modifier_book.rs:247-359 — each application allocates up to three `Vec`s, once per hit. → Write straight into the carrier's buffers.
- [ ] combat/src/combat_hooks.rs:11-25, combat_event.rs:41 — a boxed `dyn` closure with one implementation. → `Option<Rc<ModifierHooks>>`; `CombatEvents` goes.
- [ ] combat/src/recent_attackers.rs:28-50, damage_pass.rs:326-335 — `gone` and `record` through a scratch `Vec`. → One `record(source, tick, &EntityIndex)`.
- [ ] combat/src/combat_book.rs:13-16, capabilities/src/books/book_builder.rs:240-274 — the top crate builds `CombatBook` field by field and converts the assist window. → `CombatBook::new(&ModeCombatData, rate, stat)` in combat; `CombatBindings` folds in.
- [ ] docs/design/04-capabilities/combat.md:80, stats.md:24 — both promise a modifier's `on_interval` effect list, which `ModifierData` (stats/src/modifier_data.rs:16-40) refuses and `run_intervals` (combat/src/modifier_intervals.rs:281-297) never runs. → Both docs mark the list planned; the hook stays.
- [ ] package/src/mode_packages/load_check.rs:529-534 — the negative aura radius rule is in the load, the other `ModifierProblem` cases in `ModifierSpec::of`. → One home, in stats.
- [ ] combat/src/mode_combat_data.rs:41 — `DAMAGE_KIND_LIMIT` is dead. → Remove.
- [ ] Mixed field visibility: going_off.rs:5-7, interval_due.rs:6-9, wielded.rs:8-17, stats/src/stat_books.rs:11-15, refresh_scratch.rs:277-282. → One visibility per struct.
- [ ] stats/src/refresh.rs:168, docs/design/04-capabilities/stats.md (Cost) — the doc says a live-change carrier refreshes each pass; the code refreshes it with its source, the better rule. → The doc and the comment change.

### 11. Actions: the slot table and its progress

`actions`: `action_slots`, `holds`, `ready_waits`, `actions_effect`, `action_range`, `placement_data`, `action_parts`, `shots`, `shot_hits`, `fan`, `effect_list_book`, `unit_tags`; `sim`: `due_run`; `abilities`: `channels`, `toggles`, `casts`, `lib`; `combat`: `attacks`; `projectiles`: `flights`; `values`. Format, as the snapshot and the replicated types change; moves goldens.

- [ ] actions/src/action_slots/mod.rs:37-41; abilities/src/channels.rs:56-68; combat/src/attacks.rs:108-146 — `ActionSlots` holds the slot table and the progress (`underway`, `attack_target`, `interrupted`). Each attack and channel step changes it, so `hold_charges` and the four `want_passives` (holds.rs:51-52, 112) visit every active unit, and replication sends the whole slot `Vec`. → The progress is its own component, as `SlotTimes` is; the hold passes key on the table and its times.
- [ ] actions/src/holds.rs:55, sim/src/due_run.rs:19 — `hold_charges` pushes into `DueRun` on each visit, which never deduplicates: one heap entry per tick per unit until the charge is due. → Push only when the unit's next charge tick changes, or key the run by entity.
- [ ] actions/src/holds.rs:78-137, ready_waits.rs — `want_passives` keeps a `ReadyWaits` bitset and visits each marked unit four times a tick for a whole cooldown. → A due-run revisit at `ready_at`; `ReadyWaits` goes.
- [ ] actions/src/actions_effect.rs:20-23 — a timed spawn inserts `Lifespan` after the spawn and its hooks. → `Lifespan` in the spawn's `parts`.
- [ ] actions/src/action_range.rs:25-27, placement_data.rs:34, action_parts.rs:89-91 — "a distance from 0" is checked in three places, with `ActionProblem::Bounce`; `action_range` swallows the inner error. → A checked non-negative distance type in `values`, read at the boundary.
- [ ] actions/src/shots.rs:16, shot_hits.rs:16 — `Shot` and `ShotHit` repeat six fields. → One payload struct that each holds.
- [ ] actions/src/fan/mod.rs:17, delivery_data.rs:52 — at 360°, the first and last projectiles fly one direction. → Cap the spread below a full turn, or divide a full turn by `count`.
- [ ] actions/src/effect_list_book.rs:225 — `ListsOf::Launch` with `OnResolve` is `unreachable!`. → A launch's lists take a two-case hook.
- [ ] abilities/src/toggles.rs:70-71 — re-implements `Abilities::blocked`. `blocked` and its kin take five or six parameters (abilities/src/lib.rs:118, channels.rs:74, 137, casts.rs:387, projectiles/src/flights.rs:119). → A `Caster` view (tags, forced, use group) that `blocked` reads.
- [ ] Bool parameters: `ActionSlots::step_channel(…, blocked: bool, …)` (action_slots/mod.rs:486), `UnitTags::blocks(…, forced: bool, …)` (unit_tags.rs:30), `Relations::set(…, vision: bool)`; `UnitTags::properties_of`/`passes`/`blocks` take `Option<&UnitTags>` and no `self`. → Enums; methods or free fns.
- [ ] actions/src/action_slots/slot_times.rs:296 — `set_charges` is `pub` for tests. → `pub(crate)`, with an `internals` reach-in.
- [ ] Docs: `check_ranks`' doc on `number_rule` (action_data/mod.rs:210); "a unit of the mode's type" where the code resolves the action's package (effect_data.rs:35, does_spec.rs:17); "A train's `requires`" where `build` has it too (requires_data.rs:5); `Actions::load`, which exists only in tests (action_book.rs:42); "each of its places" for one (action_start.rs:16).

### 12. Units, geometry, visibility, values

`units`: `unit_core`, `unit_type_book`, `hit_handle`, `units_world`, `forced_move`; `declare`: `entity_parts`; `deliveries`; `navigation`: `forced_moves`; `geometry`: `lib`, `halves`, `grid`, `body_box`; `visibility`; `values`; `stats`: `modifier_book`. May move goldens: the spawn order changes.

- [ ] units/src/unit_core.rs:189-207, declare/src/entity_parts.rs:94 — a new unit moves between archetypes up to eight times (spawn, core, `Owner`, up to four `UnitState<S>`, `SpawnParts::give`, the deferred `Avatar`), as `KindSpawn::of` reads the core back from the entity. → `KindSpawn` takes the core values, and one `BundleWriter` write holds core, owner, states and parts.
- [ ] units/src/unit_type_book.rs:26-44, 158, 186, 217 — the book is also its builder: three `Option`s each taken once, and a late load panics. → A `UnitTypes` builder whose `finish` returns the four books.
- [ ] units/src/hit_handle.rs:20, 56 — the core names `Capability::Abilities` and `Hitboxes`; its one caller is deliveries. → The registration moves to deliveries.
- [ ] units/src/units_world.rs:276, 294 — `avatar_of` serves the harness and builds a `QueryState` each call; `held_relations` has one caller. → `avatar_of` to a gated trait; fold `held_relations`.
- [ ] units/src/forced_move.rs:300 — a garbled comment past the line width. → Rewrite it.
- [ ] deliveries/src/lib.rs:100-106 — `end_dash` takes four parameters. → A struct. navigation/src/forced_moves.rs:39 takes `Option<ResMut<Deliveries>>` for a resource every match holds. → `ResMut`.
- [ ] geometry/src/lib.rs:20, 28; grid/mod.rs:489 — `Approach` is published for one gated caller; `Grid::index` is `pub` and panics left of `min`; `mod kernel_scene` is gated mid-file. → `internals`, `pub(crate)`, at the end.
- [ ] geometry/src/halves.rs:6-21, grid/mod.rs:483 — `Halves` is a namespace struct. → Free fns.
- [ ] geometry/src/grid/mod.rs:254, 264, 509 — `strict: bool`. → An enum.
- [ ] geometry/src/body_box/mod.rs:146 — `nearest` returns `Ordering`, and every caller tests `== Less`. → A public `closer(…) -> bool`; the `Ordering` form `pub(crate)`.
- [ ] visibility/src/lib.rs:16-209 — three types in lib.rs that its children import back; no `//!`; each enum states its set three times (`NAMED`, `code`, `of_code`). → One file per type; one table per enum.
- [ ] values: four hand-made name tables (`Block` in block.rs, `RelationSet` in relation_set.rs:22-38, `EngineStat` in stat.rs, `Relation` written twice in relation.rs). → One named-enum mechanism over `enum_all!`'s `ALL`, which `ScriptEnum` also uses.
- [ ] values/src/action_id.rs:13, modifier_id.rs:13, track_id.rs:12; stats/src/modifier_book.rs:169 — `new` is `pub` for tests where siblings gate it; `ModifierBook::id_at` repeats `NameId::at`. → `NameId::at` in production; each `new` under `internals`.
- [ ] values/src/lib.rs:7-10 — a crate-wide `expect(needless_pass_by_value)` with "Bevy systems" as reason, in a crate with no system. → Remove; fix or `expect` each real case.

### 13. Net: one match run

`net`: `sim_server` (`mod`, `pending_restore`, `door`, `lobby`, `team_rooms`, `session_dir`, `local_session`, `checkpoints`, `bot_driver`, `viewer`, `offering`, `seats`, `receipts`, `sim_server_world`, `spectators`), `sim_client` (`mod`, `join_state`), `events`, `order_script`, the root modules; `capabilities`: `release/tests`; `docs/design/10-sessions.md`. After batch 7 (the writers' `close`).

- [ ] net/src/sim_server/mod.rs:304-312 — after a `LoadLatest`, `take_save_commands` goes on. Two presses in one frame load twice: two `Loaded` records, the log cut back twice. → Stop taking commands once a load is taken.
- [ ] net/src/order_script/mod.rs:108, sim_server/bot_driver.rs:130-133, sim_client/mod.rs:640-642; capabilities/src/release/tests/mod.rs:442 — bot orders look up their type by name each time an order is due, so a misspelled type shows only mid-match as `BotUnitsMissing`. The server panics with no `UnitTypeBook`, and the client skips each order silently. The name-lookup test does not scan `net`. → `BotDriver::new` and `SimClient::build` resolve each name to a `UnitType` once and refuse an unknown name at the start; the test scans `net`. Design 10 (Server bots) keeps the drop and the log only for "no living unit of it".
- [ ] net/src/sim_server/bot_driver.rs:70, 100 — each bot walks the whole `log.changes()` on each call. → Keep a cursor.
- [ ] net/src/sim_server/mod.rs:366-380, 429-434, 461-482; pending_restore.rs:32-41; door.rs:179-211; spectators/spectators_world.rs:158-167; session_dir/mod.rs:272-273 — start and restore build the match with parameter bags: seating twice, the start message three times, and the journal attached in two places, so `MatchRun.journal` is an `Option`. → Both paths produce one `MatchRun`, and one `run_match` attaches the journal and the seats.
- [ ] net/src/sim_server/lobby/mod.rs:50, mod.rs:408-412, team_rooms.rs:288-291 — visibility held in three copies and decoded twice. → Insert `MatchVisibility` once, when the terms are fixed.
- [ ] net/src/hosting.rs:4, sim_server/local_session.rs:5 — "local server" is two types. → One `Hosting`, a resource on both ends.
- [ ] net/src/sim_client/join_state/mod.rs:218, 226; sim_client/mod.rs:205-213 — `clock()` and `watching()` name one type two ways, and the client holds the clock twice. → Paired names; one clock.
- [ ] net/src/events/join_refused/error.rs:7, receipt_refused/error.rs, save_refused/error.rs, bot_units_missing.rs:20 — domain errors live in log-event modules, so `order_script` imports `events`. → Each error beside its operation.
- [ ] net/src/lib.rs:8-31 — 26 flat root modules; eleven form the wire vocabulary of net_protocol.rs:84-105. `Hosting` is client-only, `KeyFile` serves both ends from `sim_server/`, `ProcessExit` is a namespace struct, and four event folders exist only for a test file. → `net_protocol/` parents the messages and the codec; each type sits on its side.
- [ ] net/src/events/unit_died/mod.rs — logs `info` for any owned unit, where the doc says avatars only, so every Zero Hour death logs at info. → Key on avatar.
- [ ] net/src/events/input_never_applied.rs — `Unapplied` derives only `Deserialize` and repeats `Applied` by hand (sim_server/mod.rs:578-579). → The siblings' encoding, and one mapping.
- [ ] net/src/sim_server/lobby/mod.rs:57-87 — `LobbyOpening` and `LobbySetup` repeat five fields. → `LobbySetup` embeds `LobbyOpening`.
- [ ] net/src/sim_server/mod.rs:536-558 — the refuse-link block twice. → One helper.
- [ ] net/src/sim_client/join_state/mod.rs:599-601 — dead guards on a bounded product. → Compute the bound.
- [ ] Style: `exit_due(stop: bool)` (sim_server_world/mod.rs:40); wide `Viewer::view_again` (viewer.rs:156, six), `Offering::offer` (offering.rs:222), `Viewer::player` (viewer.rs:54); `as` casts (seats.rs:237, 251, receipts.rs:32, input_message.rs, sim_client/mod.rs:656); `impl Iterator` returns (`Seats::gone`, `Spectators::count`).

### 14. Session log: one type for each job

`protocol`: `session_log`, `checkpoint`, `session_result`, `server_input`, `receipt`, `input_chain`, `connect`, `server_seeds`, `delegation`, `nsec`, `random_key`, `journal`; `runner`; `verifier`. After batch 1.

- [ ] protocol/src/session_log/mod.rs:65-117 — `SessionLog` has 27 fields and seven jobs, its impl spread over three files, and a rewound log is a hidden mode (six `debug_assert!(self.to_replay.is_empty())`). During a replay, `controller`, `leaver`, `last_stamp` and `carry` answer for the log's end. → `LogEntries`, `SlotControl`, `InputSchedule` (with the spills, out of `Slot`) and `Segments`; a replay drives `InputSchedule` from `LogEntries`.
- [ ] protocol/src/session_log/session_header.rs:33, runner/src/session/session_world.rs:101, mod.rs:82 — `segment_seed` hashes `segment + 1` times at each segment's start: in the server's tick, and O(N²) in a verifier. → `ServerSeeds` checked once against the commitment.
- [ ] protocol/src/session_log/mod.rs:703, runner/src/session/mod.rs:155, 220-239 — the log signs server inputs itself; checkpoints and results are signed by the runner and verified again. → The log takes `&Signer` for each server record (`serve_checkpoint`, `serve_result`); a verifier's `Session` holds no signer, by type.
- [ ] protocol: checkpoint/mod.rs:19, 97, session_result/mod.rs:12, 55, server_input/mod.rs:17, 185, receipt/mod.rs:16-17, 57-75, input_chain/mod.rs:16-17, 101-122, connect/mod.rs:13-14, 72-80 — six signature domains as bare `&[u8]`, built three ways; each `signed_by` takes four or five parameters. V2 covers hashes only. → A `SignatureDomain` type and a `Signed` trait (`DOMAIN`, `write_body`) that `Signer` and `Verifier` read.
- [ ] protocol/src/session_log/file.rs:137, journal_records.rs:140 — `decode_within` encodes the whole log again to compare, which doubles peak memory, and each `Loaded` record decodes and verifies the whole log again. → The comparison becomes a `debug_assert`; a restore rebuilds from the records.
- [ ] runner/src/replay/mod.rs:90-99 — `check_snapshot` builds a whole match for each checkpoint. → One target, restored into again.
- [ ] verifier/src/verification/mod.rs:28-55 — the steps are joined to the file system. → A core over bytes, `&PackageStore` and a snapshot source; `file` wraps it.
- [ ] protocol/src/session_log/mod.rs:111, file.rs:101 — `position_bound` is a production field for tests. → A test override of batch 1's bound.
- [ ] protocol/src/nsec/mod.rs:43-48 — `decode` leaves the secret bytes unzeroed. → `Zeroizing`.
- [ ] Namespace structs: `Nsec` (nsec/mod.rs:14), `RandomKey` (random_key/mod.rs:5), `Verification::file` (verifier), `JournalFrames` (journal/journal_frames.rs:46-65). → Free fns; `Entropy::keypair()`.
- [ ] protocol/src/session_log — 24 `u32 as usize` casts. → One `at(u32) -> usize` helper.
- [ ] protocol/src/delegation/mod.rs:67 — rebuilds a hex form that `Value32`'s `Display` gives. protocol/Cargo.toml:32 — a `postcard` dev-dependency to name one error. → `Display`; drop the dependency.
- [ ] Test-only `pub`: `carry` (session_log/mod.rs:903), `server_inputs` (471), `revealed_seed` (357), `Verification::packages` (verifier mod.rs:57); `ResumeError::Load` (runner/src/session/error.rs:64) is built only by gated code. → Private or `internals`.

### 15. QUIC and the checks

`quic`: `io_thread`, `shared_endpoint`, `port_mapper`, `gateway`, `upnp_gateway`, `quic_endpoint`, the root; `lan-check`; `zero-hour-check`; `import`: `zero_hour/mod.rs`; `docs/design/15-quic-transport.md`. After batch 7 (`Worker`).

- [ ] quic/src/io_thread.rs:598-600, shared_endpoint.rs:542-545 — when `UdpPort::wait` fails, the thread returns and logs that the links end by their idle timeout. They never do, as only the thread calls `handle_timeout`. A panic is invisible, or shows as a poisoned mutex with a false message. → The thread's end is a state of `EndpointState`; `receive` turns it into the loss of each link, or resumes the panic.
- [ ] quic/src/port_mapper/mod.rs:474-515, gateway/mod.rs:67-124, upnp_gateway/mod.rs:50-155 — a drop waits up to about a minute (NAT-PMP exchanges, router fetches, four `igd-next` requests at 10 s), where D12 says 3.75 s plus 3 s. → Check `Stop` between requests, and D12 states the true bound.
- [ ] quic/src/upnp_gateway/mod.rs:126-149, gateway/mod.rs:108-118 — a grant followed by a failed address read leaves a mapping nobody deletes; with a permanent lease it outlives the server. → `Held` owns a mapping as soon as the router grants it.
- [ ] quic/src/quic_endpoint/mod.rs:414-425 — a server link spawns, then gets `RemoteId` and `Linked`. → One bundle.
- [ ] quic/src/quic_endpoint/mod.rs:240-250, 528-546 — bind, address and error mapping written twice. → One constructor.
- [ ] quic/src/quic_endpoint/mod.rs:189-211, 234 — `cid_len` in production signatures for one test. → `CID_LEN`; a test reach-in.
- [ ] quic/src/gateway/mod.rs:53-63, upnp_gateway/mod.rs:79-86 — "this host's address facing X" twice. → One fn.
- [ ] quic/src/lib.rs:11-47 — the port map's twelve modules flat beside the transport; `MapError` in the shared error file. → A `port_map/` tree with its own `error.rs`.
- [ ] checks/lan-check/src/lan_match.rs:610-616 — `free_port` releases the port before the server binds it. → Bind to 0 and read the port from `Listening`.
- [ ] checks/lan-check/src/lan_match.rs:396-457, process_group.rs:23-78 — roles by position in a `Vec`. → Key by `Process`.
- [ ] checks/lan-check/src/lan_match.rs:478-561 — each 50 ms, every log is read and parsed whole. → Keep an offset.
- [ ] checks/zero-hour-check/src/main.rs:265, 320; import/src/zero_hour/mod.rs:100-118 — the map list comes from a fault report's keys. → `Imported` lists the maps it wrote.
- [ ] checks/zero-hour-check/src/main.rs:218-241, failure.rs:10 — four pins written twice; main and `failure` import each other. → A `pins.rs`.
- [ ] checks/zero-hour-check/src/skirmish_play.rs:251-270, start_drive.rs:20-125, main.rs:212-506 — some missing parts panic, others fail; type names, the owned-units query and the stepper written twice (one counts the dead); `run` is 250 lines; `read_toml` takes a parser all callers pass alike. → One play helper that returns `Failure`; `run` split by check group.

### 16. Packages

`package`: `file_index`, `package_walk`, `mode_packages`, `files`, `error`, `texts`; `package-disk`: `package_writer`, `package_dir`; `client-files`; `server`: `main`, `opening`; `client`: `action_names`, `item_names`.

- [ ] package/src/file_index/mod.rs:49-51, 91, 148 — `new` refuses only the spelling `package.index` and does not check the bounds that `decode` checks. An index that lists `PACKAGE.INDEX`, a file and a directory of one name, or a file past 2 GiB gets a fingerprint and then fails every read. → `new` holds every rule: case-folded names with its own path, no clash of a file and a directory, the bounds. `MAX_FILE_LEN` becomes private.
- [ ] package-disk/src/package_writer.rs:46-50, package/src/package_walk.rs:50 — the writer records the row before the write, and the walk replaces a repeated path. A refused second write leaves a row for bytes not on disk, and a later hard link copies it. → The walk refuses a repeated path; the writer commits after the write.
- [ ] package/src/mode_packages/load_check.rs:140, mod.rs:391-408, dependent.rs:43-72 — the API version is checked after every file is parsed and every script compiled, so a newer package fails as `Data`, not `OtherApi`, against design 03. → Check it as each manifest is read.
- [ ] package/src/file_index/mod.rs:21, package_reader.rs:15, package.rs:72 — the index holds its rows and their encoding (up to 64 MiB), and each reader clone copies both. → `Arc<FileIndex>`; the builder hands the encoding to the writer.
- [ ] client-files/src/{terrain_file.rs:148, map_lights.rs:36, client_objects.rs:26, map_conditions.rs:15} — `path(map: &str) -> String`, re-parsed with `expect` by each caller; `heights.bin`, `client/units.toml` and `client/materials/` are spelled by hand in the client, the import and the check. → `path(&MapName) -> PackagePath` on each type, `HeightGrid`, `ClientUnits` and `MaterialFile` included.
- [ ] client-files/src/material_file.rs:9 — no check of its 0-to-1 values, where its siblings check; NaN reaches Bevy (client/src/view/file_material.rs:64). → A checked `Deserialize`.
- [ ] client-files/src/client_units.rs:54 — the generic crate names Zero Hour's `SparseMatchFinder`. → Remove the name.
- [ ] package/src/files/dependency.rs:7, package-disk/src/package_dir/mod.rs:113 — a dependency path is a raw string joined as an OS path; an absolute path replaces the root. → A checked relative-path type.
- [ ] package-disk/src/package_dir/mod.rs:39-193 — the memory tree is a variant of the disk type, so production branches on a test case. → `PackageDir` disk-only; the harness's tree implements `PackageSource` behind `internals`.
- [ ] package-disk/src/package_dir/mod.rs:87, package/src/mode_packages/mod.rs:67 — both apps read the index twice; `choose_map`'s `map` is always `None`. → `mode_packages(map: Option<MapName>)` reads once.
- [ ] package/src/error/mod.rs:45, package_walk.rs:44, package_store.rs:103 — `ContentError::Io` makes a disk failure read as "no package". → The walk returns `io::Error`; package-disk maps it.
- [ ] package/src/texts.rs:82; client/src/action_names.rs:41, item_names.rs:41 — `text` takes `&str` for a `PackageName` key, and the client builds a full `Texts` twice. → `&PackageName`; one `Texts`, shared.
- [ ] package/src/files/avatar_file.rs, lib.rs:44 — `pub` for one harness test. → `pub(crate)`.
- [ ] server/src/main.rs:105-171 — five refusals repeat the log and the exit inline beside a `refused` helper. → A typed start error, logged in one place.
- [ ] server/src/opening/mod.rs:131-132 — `Opening::find` reads `SystemTime::now()`. → Take `now`.

### 17. Zero Hour import: bugs and boundaries

`import`: `zero_hour` (`game_version`, `install`, `archive_path`, `model_writer`, `model_import`, `production_import`, `terrain`, `terrain_cells`, `game_data_ini`, `map_file`, `hlod`, `mod.rs`), `texture`, `lib`. After batch 16 (typed paths). Moves the Zero Hour fingerprints where an item changes written bytes.

- [ ] import/src/zero_hour/game_version.rs:193-196, install.rs:94-98 — `identify` collapses names by case and the last hash wins; `Install::open` keeps both and reads the first. A modified `INIZH.BIG` beside the genuine `INIZH.big` passes the check, and the import reads the modified bytes. → Refuse two archives with one `ArchivePath`.
- [ ] import/src/zero_hour/archive_path.rs:18, model_writer.rs:95-116, mod.rs:167, model_import.rs:610-613 — package paths come from names of the original by `format!` and `expect`; a `:`, `..`, `aux.dds` or `/` panics or breaks the `"../"` texture URI. → One checked conversion that returns `ZeroHourError`; the URI relative to the model's `PackagePath`.
- [ ] import/src/zero_hour/production_import.rs:318-329 — `windup_ms` overflows for a huge `BuildTime`. → `checked_mul` within the game's range; drop it as `BadTime`.
- [ ] import/src/zero_hour/terrain_cells/mod.rs:309, 346, terrain.rs:105-155 — `width * TILE` on an unbounded class width. → `Terrain::new` refuses a width past the atlas.
- [ ] import/src/zero_hour/game_data_ini.rs:136 — `tan` through libm enters the fingerprint, which must be equal on every OS (D4). → An exact `f32` literal with its derivation.
- [ ] import/src/zero_hour/map_file.rs:422-435 — `WorldInfo`'s version is not checked. → `check_version`.
- [ ] import/src/zero_hour/hlod.rs:59-62 — two LOD arrays in one level reads as `Levels(1)`; aggregate arrays are skipped with no report. → Their own variant; report aggregates.
- [ ] import/src/zero_hour/mod.rs:100-118, 216-219, 337-340 — `dropped_waypoints`, `missing_textures`, `unliving` and `wounded` hold names as strings; `untextured_cells` keeps zero counts where `dropped_waypoints` does not; a non-finite `PlacementViewAngle` is dropped silently. → Typed names, one rule, a refusal.
- [ ] import/src/texture/tga_file.rs:7-9, 128 — the general texture module holds Zero Hour's alpha rule. → A parameter that the importer passes.
- [ ] import/src/lib.rs:12 — publishes `Short`. → Crate-private.

### 18. Zero Hour import: structure

`import`: `zero_hour` (the readers, the conversions, `error`, `import_names`, `imported_type`, `rules_package`, the INI readers, `model_import`), `texture`; `mode`: `mode_data`; `capabilities`: `books/unit_type_data`. After batch 17. No fingerprint moves.

- [ ] import/src/zero_hour/mod.rs:179-293, import_names.rs:20-25, production_import.rs:125, combat_import.rs:139-144 — one mutable `ImportNames` through every stage; moving combat above production drops weapons. → The closed set of unit types first, immutable; the stages return their additions.
- [ ] import/src/zero_hour/imported_type.rs:18-68, import_names.rs:39-81, rules_package.rs:38-166; mode/src/mode_data.rs:26, capabilities/src/books/unit_type_data.rs:20 — `units.toml`, `mode.toml` and the manifest are written by hand-mirrored structs and `format!` templates, as the engine types derive `Deserialize` only. → The engine types derive `Serialize`; the importer builds them, as it builds `MapData`.
- [ ] import/src/zero_hour/{terrain,game_data,multiplayer,player_template,command_set,command_button,armor,weapon,locomotor}_ini.rs — nine copies of one block grammar; `multiplayer_ini` differs. → One `IniText` block iterator.
- [ ] import/src/zero_hour/weapon_ini.rs:147, 168-188, armor_ini.rs:50, 144; combat_import.rs:121-208, imported_type.rs:138; error.rs:100, 149; rules_package.rs:31, map_import.rs:126 — `affects` parses flags by its own rules; damage types are strings and indices, resolved at parse by armors and later by weapons; the armor id is derived twice with two errors; names of the original follow three rules; `PLAYERS` and `SLOTS` are one constant. → A `DamageType` enum at the boundary; `flags()` and one apply; one naming function; one constant.
- [ ] import/src/zero_hour/ (45 flat children), w3d_mesh.rs:1, 510-560, map_file.rs:8, 148, error.rs:202-307 — the readers of the original depend on output types (`Blend`, `LookConditions`); `MapError` mixes reading and conversion; `Lighting` means two causes. → `zero_hour/format/{big, ref_pack, map, w3d, ini, csf}` with their own errors, below the conversion modules.
- [ ] import/src/zero_hour/map_file.rs:274, 325, mod.rs:203-206, map_import.rs:157-160, 299, 365 — `TextureClass { texture: None }` is patched later; `columns` repeats `width`; the grid flips twice; the cliffs are read unflipped. → The reader keeps class names; the conversion pairs and flips once.
- [ ] import/src/zero_hour/model_import.rs — 691 production lines with three concerns, plus a 1,010-line test mod; `detail_blend()` computed twice per group. → `model_import/{mod.rs, model_builder.rs, w3d_material.rs, tests.rs}`.
- [ ] import/src/zero_hour/model_import.rs:103-198 — `convert` takes `&mut ModelAssets` over `&Install`, so the tests write `.big` files and the borrow forces clones. → Resolve first; convert plain `&W3dFile` and `&Hierarchy`.
- [ ] import/src/zero_hour/mod.rs:321-440, model_writer.rs:17 — `write_models`, `look` and `models` are receiverless fns of `ZeroHour`, generic over one closure type that re-parses paths. → A look-writer module; a concrete writer that takes `PackagePath`.
- [ ] import/src/zero_hour/chunk_reader.rs:41-63, w3d_fields.rs:111-125, w3d_chunks.rs:42-49 — forwarding methods that `?` already gives; a header parsed by hand. → The readers hold `ByteCursor`.
- [ ] Namespace structs and receiverless fns: `RefPack` (ref_pack.rs:9), `Srgb` (texture/srgb.rs:6), `Pose::y_up` and kin (pose.rs:67-97), `MapFile::i16`/`index`/`entry` (map_file.rs:483-494). → Free fns or methods.
- [ ] import/src/zero_hour/mod.rs:138-149, game_version.rs:11-28, 209 — `open(root, known)` and `pub` fields for tests; `identify` panics on an empty list. → `open(root)`; a gated constructor.

### 19. Script calls and arguments

`script`: `script_batch`, `frame`, `ctx`, `view`, `view_api`, `script_arg`, `id_args`, `state_value`, `num_api`, `ctx_fn`, `error`, small files; `mode`: `calls`, `lib`, `mode_spec`, `mode_api`, `mode_hooks`, `choice_id`, `marker_tag`; `orders`: `thinking`; `abilities`, `stats`, `vision`, `navigation`, `combat` (their `*_api.rs`, `combat_column`, `modifier_handle`). After batches 6 and 9.

- [ ] script/src/scripts/view/mod.rs:117-135, view_names.rs:124; abilities_api.rs:114, stats_api.rs:91, vision_api.rs:45, navigation_effect.rs:85, mode_api.rs:130-700, combat_column.rs:118 — no `ScriptArg` for times or player slots. Each capability picks `ticks`, `lasting` or `window` by hand, `combat_column` repeats the negative check, and `mode_api` resolves players about eight times. `reduce_cooldown` converts by `ticks`, which gives at least one tick, so a cut of 0 ms cuts a tick. → `ScriptTicks` (a cut, rounded down, 0 for 0), `ScriptLasting` and a `PlayerSlot` argument; the `View` resolvers become private.
- [ ] script/src/scripts/script_batch.rs:33-84, frame/mod.rs:109-120; mode/src/calls.rs:74, 136, 146; orders/src/thinking.rs:126 — two call flows; mode and orders re-implement "a spent pool waits"; purity is a `bool` outside `CallStart`. → One entry that begins, calls, and applies or records, returning `CallError` with a "pool spent" case. Purity is an enum in `CallStart`.
- [ ] docs/design/02-engine-core.md:301 — "A call that finds its pool spent waits" covers every pool, but `hook_call` (abilities, deliveries, modifier hooks) records a failure. → Design 02 says an action's, a delivery's or a modifier's hook that finds its pool spent fails as over budget, and the mode's and the AI's calls wait. The one call entry above gives that rule as each caller's handling of the "pool spent" case.
- [ ] docs/design/02-engine-core.md:295-297 — the pool table puts inputs in the `mode` pool; the code charges `on_mode_input` to the sender's `Pool::Player(slot)` (mode/src/mode_hooks.rs:84), so one player's inputs cannot starve the mode's timers and deaths. → The table moves inputs to the player's pool; the code stays.
- [ ] script/src/scripts/ctx.rs:29, 102-109; mode/src/lib.rs:281-284, mode_spec.rs:56-58 — `Ctx` has a `dyn Any` slot for the mode alone, and the mode keeps `ModeSpec` three times. → The mode reads its spec through its `ModeCall` part; the slot and `MatchMode` go.
- [ ] script/src/scripts/state_value.rs:39, 74 — each read of a text state field allocates a new `ImmutableString`, and each write an `Arc<str>`. → One shared text type for state and scripts.
- [ ] script/src/script_host/num_api.rs:70, ctx_fn.rs:156 — `clamp` has only the `(Num, Num, Num)` form, so `x.clamp(0, 10)` fails; `CtxFn` registers one integer form. → Integer forms for `clamp`; `CtxFn` and `MethodFn` register every form or assert at most one `Num`.
- [ ] script/src/scripts/error.rs:47, script_host/error.rs:76, state_value.rs:35 — one failure, three cases. → `NumError::IntegerBeyondNum` alone.
- [ ] script/src/scripts/view_api.rs:57-72, view/mod.rs:222, view_names.rs:54 — an id with no name gives an error, `()` or a panic. → `ViewNames::name` is infallible.
- [ ] script/src/scripts/view_api.rs:158, script_arg.rs:157, 190; mode/src/choice_id.rs:22, marker_tag.rs:24 — `ScriptArg` impls stand away from their types; two `TextArg` impls by hand where siblings use `name_script_arg!`. → Beside each type; `DamageKind` in `id_args.rs`; the macro.
- [ ] stats/src/modifier_handle.rs (`source`) — repeats `View::unit_value`. → Call it.
- [ ] Small items: an orphan doc on frame/mod.rs:36; `SyncTo::ALL`, `ParamScope::index` and `StateSchema::new` by hand where siblings use `enum_all!`; no const assert `Hook::ALL.len() <= 32` (hook_set/mod.rs:26); mixed field visibility in `TargetQuery` (target_index.rs:23-29); a doc that names a missing type (state_decl/mod.rs:32); `.unwrap_or_default()` for a discarded `Result` (script_host/mod.rs:89).

### 20. Small capabilities: one shape for siblings

`orders`, `areas`, `projectiles`, `progression`, `interaction`, `items`, `production`: `production_orders`; `players`: `tick_commands`; `navigation`: `route`; `actions`: `targets`; `capabilities`: `book_builder`. After batches 11 and 19.

- [ ] orders/src/orders_api.rs:16-71, interaction/src/interaction_api.rs:12-16 — `ctx.order_action`, `unit.inside` and `unit.passengers` are in the docs, and neither bound nor planned. → Each name a capability doc lists is bound or `.plan`ned, and a test reads the docs' Script API sections to hold it.
- [ ] orders/src/unit_order.rs:76, 111, 171-246; navigation/src/route.rs:77 — the orders effect is `UnitOrder`, not `OrdersEffect`; its `apply` shares `Effect::apply`'s name; it writes navigation's `Route`, `Progress`, `PathWalker` and production's `Builder`, `Gatherer`, and must pair `route.ask` with `progress.restart()`. → `OrdersEffect`; asks through contract parts of `units`; `Route::ask` restarts the progress.
- [ ] orders, items, progression, production — `OrderInputs`/`PlayerOrders` against `<X>Orders`; `OrdersSet::Orders` against `<X>Set::Commands`. → One naming.
- [ ] orders/src/error.rs:10, ai.rs:30 — `AiError::TimeTooLarge` is its own case. → `Time(#[from] TimeTooLarge)`.
- [ ] items/src/trade_orders.rs:53-61, progression/src/learn_orders.rs:30-39, orders/src/order_inputs.rs:317-324, production/src/production_orders.rs:73 — each resolves id → entity → query → `controls(owner)`. → A `TickCommand` method in `players` that yields the controlled entities.
- [ ] areas/src/lib.rs:136-144, areas_effect.rs:15, projectiles/src/lib.rs:140, 199, launches.rs, projectiles_api.rs:63-66 — the two deliveries differ: a raw scratch against a wrapped one, a `pub` effect against a `pub(crate)` one, a six-parameter `push`; "launch" means a listed effect and a spawn queue; homing is held in the shape and in the spec. → One shape: a wrapped scratch, a `pub(crate)` effect, `push(launch)`; the queue renamed; homing read from the delivery.
- [ ] projectiles/src/lib.rs:104-107, areas/src/lib.rs:108-110 — `take_shots`, `launch` and `land` run on the client, which never fills them. → `cx.server`.
- [ ] projectiles/src/lib.rs:47, flying.rs:61; actions/src/targets.rs:22 — "It builds on combat" is false; the one use of stats is a `Without<Life>` filter, forced as `Targets` requires `&Life`. → Fix the doc; disjoint by `With<DeliveryEntity>`.
- [ ] progression/src/progression_column.rs:26, 54-58, track_book.rs:34, 103 — a `book` field refreshed and never read, `TrackBook::same` for it, and a doc that describes it. → Remove.
- [ ] progression/src/learning.rs:46; client/src/hud/mod.rs:92, 581 — charges come from progression's `Learning`, so a mode without progression shows none. → An actions-side reader of charges.
- [ ] items/src/shopping.rs:31, 105 — `ShopUnits` is every living unit, and the client calls `nearest_for` twice a frame. → A shop-only index.
- [ ] orders/src/thinking.rs:71-90 — `think` walks every AI unit each tick for the few due. → A due run.
- [ ] items/src/inventory.rs:58, capabilities/src/books/book_builder.rs:1017 — items extends a spawn's slots, and the top crate builds interaction's use slot. → `UseSlot::extend_slots`.
- [ ] areas/src/lib.rs:118, projectiles/src/lib.rs:115, production/src/lib.rs:223-237 — engine tag numbers are assigned by hand per crate to match the module list. → `declare` assigns them in declaration order.

### 21. Client

`client`: `hud`, `view` (`mod`, `unit_models`, `client_data`, `terrain_meshes`, `camera_rig`), `command_panel`, `shop_panel`, `inventory_bar`, `orders`, `build_preview`, `selecting`, `main`, `args`, `local_keys`, `ui_style`. After batches 16 and 20.

- [ ] client/src/hud/mod.rs:442-456 — `face_camera` gives each stand the camera's rotation as its local rotation, but a stand is a child of a root marked `Turns` (view/mod.rs:369, 780-783). A unit's life bar turns with it and is edge-on at 90°. → The stand's world rotation is the camera's (remove the root's yaw), and the system runs only when the camera, a yaw or the stands change.
- [ ] client/src/orders.rs:153-166 — Q/W/E/R cast slots 0 to 3 whatever their kind, where item keys find their slots by kind. A mode that declares `weapon` first fires the weapon on Q. → The ability keys find the first four ability slots (design 13 D11).
- [ ] client/src/view/unit_models.rs:186-196 — a texture's sampler is set on the shared image, keyed by path, so the first load's wrap mode wins. → The sampler goes with the material: one image asset per sampler.
- [ ] client/src/command_panel.rs:192-290, inventory_bar.rs:98-136, shop_panel.rs:121-188, view/mod.rs:599-626, 720 — each frame the panels walk the selection and reformat every line, and `lean` and `ring_selection` walk every unit. → Each is gated on `Changed`/`Added` of what it reads; per frame only progress and windups.
- [ ] client/src/view/client_data/mod.rs:29-165 — `ClientData` lives the whole match but holds startup inputs (the whole `TerrainFile`, 3.8 MB), copies the flips into `GroundHeights`, and keeps two parallel `Vec`s by index; `toml` takes a parser that every caller passes alike, and a `&str` path that it parses each time. → A per-package record; startup inputs taken by their systems; `GroundHeights` alone stays.
- [ ] client/src/main.rs:206-258, action_names.rs:24, item_names.rs:88, view/terrain_view.rs:28, local_keys.rs:109, command_panel.rs:63 — four plugins each hold `Arc<ModePackages>` in a private resource (two identical `NamedPackages`); `add_ends` rebuilds the resource names the match holds in `ModeNames`. → One packages resource; read `ModeNames`.
- [ ] client/src/view/unit_models.rs:90-110 — `place_objects` resolves the look of each object instance (a `Vec`, an `Arc` copy, a path `String`, a lookup). → Resolve once per look, as `UnitLooks` does.
- [ ] client/src/view/mod.rs:39, 707-761, hud/mod.rs:628-663 — two ground-ring systems with their own meshes and trackers; `view` depends on `selection` for one of them. → One ring overlay in the HUD; cut view → selection.
- [ ] client/src/command_panel.rs:65-308, shop_panel.rs:40-218 — the panels repeat their types and spawns, and read the player's amount two ways. → One panel widget in `ui_style`; one accessor (the own slot).
- [ ] client/src/view/camera_rig.rs:77-80, selecting.rs:53 — two click rules. → One.
- [ ] client/src/view/mod.rs:69, 289-295 — a map with no terrain draws the MOBA lane's 44 × 20 m plane, and the doc says `View` owns the camera. → Size from the map's bounds; fix the doc.
- [ ] client/src/inventory_bar.rs:53, 118 — `SLOTS = 6` and the labels restate `ITEM_KEYS`. → Derive them.
- [ ] client/src/hud/mod.rs:248 — `Fills` claims every mesh transform. → A `GaugeFill` marker.
- [ ] client/src/view/mod.rs:333, build_preview.rs:127, command_panel.rs:316 — `Option<Res<UnitLooks>>` for a resource that always exists. → `Res`.
- [ ] client/src/command_panel.rs (about line 508) — `site_percent`'s `expect` names an invariant it does not test. → Assert the relation, or clamp.
- [ ] client/src/args/mod.rs:21, 151 — `team` returns a `&'static str` error; `spectate: bool` selects behavior. → An error enum; parse the role at the boundary.
- [ ] Wide signatures: `TerrainMesh::quad` (terrain_meshes.rs:172), `press_pan` (camera_rig.rs:173), `ground_under` (239), `Shown::fill` (command_panel.rs:414), `cancel` (379), `spawn_row` (shop_panel.rs:191). → Param structs.
- [ ] client/src/build_preview.rs:41-45, 158-164 — design 18 D7 draws the building's look red where its box overlaps; the code leaves the look plain and draws a separate tinted footprint box. → The pipeline gains a general per-drawing tint over the look's materials, which any game may use; the preview tints the look, and the footprint box goes.

### 22. Published surfaces

Every capability crate's `lib.rs`; `net`. Last of the code batches, as the batches above move items. Design 02: "an item that another crate uses becomes `pub`; the rest stays `pub(crate)`".

- [ ] Narrow to `pub(crate)` and remove from `lib.rs`: abilities `AbilitiesApi`, `Casts`; areas `AreaHolds`, `AreaLaunch`, `AreaLaunches`, `AreasApi`, `AreasEffect`; interaction `InteractionApi`; items `CarrierSlots`, `ItemHolds`, `ItemsApi`; orders `Destinations`, `OrderInputs`, `PlayerOrders`, `OrdersApi`, `Thinking`, `OrderedUnit`; progression `LearnOrders`, `ProgressionApi`, `ProgressionColumn`, `ProgressionEffect`, `Points::gain`; projectiles `LaunchPayload`, `Launches`, `Launching`, `Payload`, `ProjectilesApi`; combat `CombatApi`, `CombatEffect`, `CombatEvent`, `CombatEvents`, `CombatHooks`, `ModifierHooks`; production `Construction`, `GatherLoop`, `Trains`, `ProductionOrders`, `ProductionApi`, `ProductionColumn`, `Step`, `Start`, `BuildSpec`, `Required`, `Progressed`, `SupplyUnit`, `UnitSupply`; mode `Calls`, `ModeApi`, `ModeCall`, `ModeHooks`, `MatchMode`, `ModeSpec`, `Choices`, `ModeRefusal`, `SaveAsked`, `DeathCall`, `UnansweredDeaths`, `Timer`, `GameMap`, `Marker`, `MarkerInfo`. Each name a batch above already removed or renamed drops from this list.
- [ ] Behind `internals`: combat `DamagePass::restore` (damage_pass.rs:397), `Attacks::strike` (attacks.rs:230); net `JournalWatch`, `InputChannel`. To `pub(crate)`: net `SessionJournal`, `SnapshotDir`, `SessionFiles` and `RestoredSession.files`; net `SessionDir::create/start/restore/remove/publish`.
- [ ] Spec field visibility: `AreaSpec` all `pub`, `ProjectileSpec` mixed, `UseSpec` and `Shop` private with accessors. → One rule.
- [ ] A release test: each module's effect is `<Module>Effect`, and each `pub use` item is named by another crate.

### 23. Design documents

The designs that no batch above changes. Each names where code and design disagree, and which one changes.

- [ ] docs/design/02-engine-core.md, Dependencies and the `common` row — `common` depends on `blake3` and `sha2`, and holds the hashers, `Version`, `MapName`, `AfterLeave`, `Secret` and `enum_all!`. → The design changes.
- [ ] docs/design/02-engine-core.md:263 — lists the package index under Postcard; it is a text manifest. → The design changes.
- [ ] docs/design/02-engine-core.md:154 — lists `items` in `campfire-mode`'s Uses; `Mode::USES` has none. → The design changes.
- [ ] docs/design/04-capabilities/production.md — Cost lists supply passes only in a train's tick or a view read; Network requires a derivation each tick. → One rule, after batch 9.
