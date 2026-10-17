# Questions of the architecture plan

Decisions that the plan in `review-architecture.md` cannot take alone. Each names its item, the options, the recommendation and what waits for it.

## Q1. Batch 1.1, transport bounds: where the fix goes

- **Decided (2026-10-17):** Fix the fork, in a branch cut from upstream Lightyear's master, which the user proposes upstream as a PR.

- **Item:** 1.1, N-1 and N-2. Lightyear's fragment receiver allocates `num_fragments × fragment_size` from the peer's count before any check, and a link that did not join uses every reliable channel with no budget (fork `xorza/lightyear` at `54e6be2`, `crates/transport/transport/src/channel/fragments/receive.rs:137-147`, `channel/receive.rs:179-215`).
- **Why it waits:** the fix belongs in the fork's transport crate, a repository apart from this one. This plan may commit and push only on its own branch of `campfire`.
- **Options:**
  1. **Fix the fork (recommended).** A branch of `xorza/lightyear` gives each channel a largest message size, refuses a larger fragment count before it allocates, caps the half-built messages for each link and channel, and expires them on reliable channels too, as ENet and yojimbo check before they allocate. `net_protocol` declares each message's largest encoding, and this repository pins the fork by `rev`. Cost: a change in the fork, which the user commits and pushes, or allows.
  2. **A guard in `quic` or `net`.** It reads Lightyear's packet header from each datagram before Lightyear does, and drops one whose fragment count passes a bound. Cost: a second reader of Lightyear's wire format, which breaks when the fork changes it.
  3. **Vendor the transport crate.** Copy `lightyear_transport` into the workspace as a path patch and fix it there. Cost: a copy of an outside crate to keep in step with the fork.
- **Blocked:** batch 1.1 alone. No later batch needs it.

## Q2. Batch 3.4, a script's print: where it goes

- **Decided (2026-10-17):** A print is a message of the match.

- **Item:** 3.4, B-3. The rule "no crate of the sim's closure requires `store`, `tracing` or `log`" holds for every crate but `campfire-script`, whose debug build passes a script's `print` and `debug` to `tracing::debug!` (`capabilities/core/script/src/script_host/mod.rs`, `ScriptHost`). The test `the_sim_reads_no_file_and_logs_nothing_and_the_protocol_reads_no_file` in `rules` names that one exception.
- **Why it waits:** the choice decides what a scripter sees of a print, and no design document says.
- **Options:**
  1. **A print is a message of the match (recommended).** The script host writes each print into a `Messages<ScriptPrinted>`, which the app reads and logs, as OpenRA's Lua `Media.Debug` reaches the game's own debug output. The sim then logs nothing, and the exception goes. Cost: one message type, and a reader in the server and the client.
  2. **Keep the exception.** A debug build's print goes to `tracing` at the debug level, and the test names it. Cost: the sim logs, in a debug build, the one place it does.
  3. **No print.** Rhai's `print` and `debug` do nothing in every build. Cost: a scripter loses print debugging.
- **Blocked:** nothing. The test holds every other crate now.

## Q3. Batch 4.1, the engine tags' numbers: given in order, or written by hand

- **Decided (2026-10-17):** Keep the hand numbers.

- **Item:** 4.1, RW 20, "`declare` assigns engine tag numbers in declaration order". Each module writes its tags' numbers by hand, as `Projectiles::TAG = EngineTag::new(2, …)`, and `Release::engine_tags` asserts that each number is its place in the declaration order.
- **Why it waits:** a number given at run time can no longer be a constant, and a tag constant is what the hot filters read, as `UnitTags::has(Dead::TAG)`; the choice trades a hand number, which an assertion already checks, for a read through a resource on each use.
- **Options:**
  1. **Keep the hand numbers (recommended).** The assertion fails a wrong number at the first build, and the constants stay free to read. Cost: a new module's author picks the next number, which the assertion then checks.
  2. **Numbers at run time.** The declarations walk gives each tag its number into `TagBook`, and each use reads it there. Cost: a resource read where a constant is now, in filters that run for each unit.
  3. **One list in `declare`.** An enum of every engine tag, whose discriminant is its number. Cost: a lower layer names each capability's tag, against "The core names no capability".
- **Blocked:** nothing. The rest of 4.1 is done without it.

## Q4. Batch 4.5, the data kinds: one enum, or rows each module declares

- **Decided (2026-10-17):** Keep the closed enum.

- **Item:** 4.5, E-6, "each module declares its `DataKind` rows". `DataKind` in `state-types`, a base crate, is a closed enum of 18 kinds, and 11 of them name a capability's concept, as `Life`, `Inventory` and `Supply`. Each of the 74 state types states its kind as a constant, `const KIND: DataKind`, and the visibility tables and the server's filter read a kind as its index.
- **Why it waits:** the choice is the one of Q3 for the engine tags. A kind that a module declares gets its number at run time or by hand, and a kind is what the filter of each sent component reads.
- **Options:**
  1. **Keep the closed enum (recommended).** The kinds stay one list in `state-types`, each with its doc, and the plan's bullet goes. Cost: a base crate names concepts of the capabilities above it, as E-6 says, and a new capability's kind edits the base crate.
  2. **Rows with hand numbers.** Each module declares its kinds as constants, `DataKind::new(3, "inventory")`, and the release asserts that each number is its place in the declaration order, as `Release::engine_tags` does for the tags. Cost: the 74 state types and the visibility tables change, and the visibility format reads a kind by its name only.
  3. **Rows numbered at run time.** The declarations walk numbers each kind, and each state type reads its kind's number through a resource. Cost: a read through a resource where a constant is now, in the filter of each sent component.
- **Blocked:** the third bullet of 4.5 alone. Its first two bullets and the removal of the unbuilt capabilities do not wait for it.

## Q5. Batch 4.5, the parts by type: `ClipShots`, `RecentAttackers` and `SpawnPoint`

- **Decided (2026-10-17):** Keep the three parts as they are.

- **Item:** 4.5, K-11 and C-7, "per-type predicates give `ClipShots`, `RecentAttackers` (with its entries bounded by the window), `StatusTags` and `SpawnPoint`". `StatusTags` is done: production gives it to its gatherers, and a site gets its own at its spawn. The other three each meet a fact the plan does not name.
  - **`ClipShots`** is required by `ActionSlots`, so a grant of slots during play brings it. A mode effect's grant (`mode/src/mode_effect.rs`, `ModeEffect::Grant`) can add a weapon with a clip to a unit of any type, and the attacks query takes `&mut ClipShots`, so a unit of a type with no clip weapon would lose its attacks after such a grant.
  - **`RecentAttackers`** is read by scripts with a window of their own, from a type's params, as `ally.recent_attackers(unit.params.help_window_ms)` in every test mode's AI. No static window bounds its entries, and the assist window alone would cut what such a read sees.
  - **`SpawnPoint`** has three readers, not one: combat's respawn, the orders' reset, which expects one on every unit the mode spawned (`orders/src/unit_order.rs:158`), and the script field `spawn_pos`. In the test modes, nearly every unit respawns or thinks, so it would keep the part.
- **Options:**
  1. **Keep the three as they are (recommended).** `ClipShots` stays with `ActionSlots`, `RecentAttackers` with a type that can die, and `SpawnPoint` on every unit placed at a spawn, and the bullet goes. Cost: an empty `ClipShots` and a `SpawnPoint` on units that never read them.
  2. **Predicates with their paths.** A grant of slots that adds a clip weapon inserts `ClipShots`; the script API caps a `recent_attackers` window, and the entries are bounded by the cap and the assist window; a book of the types that keep their spawn, which combat's respawn and the orders' AI extend, gives `SpawnPoint`. Cost: a change of the script API's `recent_attackers`, a structural change at such a grant, and a book that two capabilities extend for one part.
- **Blocked:** the first bullet of 4.5 alone.

## Q6. Batch 5.2, one effect registry: how an open set of effects reads and resolves

- **Decided (2026-10-17):** A trait object for each effect.

- **Item:** 5.2, C-2 and S-8, "each capability declares an effect once, with its data key, script name and argument types". Today `DoesData` and `CapabilityDoesSpec` are closed enums in `actions` (`effect_data.rs`, `capability_does_spec.rs`), and the core reads each effect for four things: its numbers, for the params' checks; its duration, for the whole-millisecond rule; the modifiers it names, for the ways that apply them; and the lists it holds, as a launch's `on_hit` and `on_end`.
- **Why it waits:** an open set needs a type for an effect the core does not know, and a place for its resolved form, and the plan names neither. A list reads inside `[actions]`, a section that today reads with no registry, so either choice also gives a section the release's readers as it reads.
- **Options:**
  1. **A trait object for each effect (recommended).** `actions` defines `trait ListedEffect` with the four reads and `resolve`, and a list holds `Box<dyn ListedEffect>`, as `Problem` holds a module's error; each capability registers its parser by data key, and its `ctx` call builds the same value. A resolved effect is the capability's own, in a book of its own by index, which its `EffectQueues` callback reads. Cost: a box per listed effect at the load, and `Section` reads gain the release's readers.
  2. **One enum of data, each capability's resolve.** `DoesData` stays a closed enum of data, and only the resolved specs and the queues move to the capabilities. Cost: the core still names each capability's effect data, so C-2 stays in part.
  3. **Wait for 8.1.** The `ctx` calls come from the declarations only once forms carry types (8.1), so 5.2 waits until then. Cost: 5.2 moves after Phase 8.
- **Blocked:** batch 5.2 alone.

## Q7. Batch 5.3, one action pipeline: a slice of its own, with its proposal first

- **Decided (2026-10-17):** A proposal first.

- **Item:** 5.3, K-17, C-3 and E-5, and with it the first bullet of 5.1 (C-1), which needs 5.3's callbacks before `Weapon` leaves the core.
- **Why it waits:** the batch flips a layer. With `actions` free of `stats`, `stats` sits above `actions`, so the modifier ways that 4.4 put in `actions` (`modifier_ways.rs`, `modifier_appliers.rs`) move to `stats`, and every pool cost, stat read, param table and level that `actions` takes from `stats` today (26 imports) goes through a contract in `units`. The four loops (`attacks.rs` 520 lines, `casts.rs` 500, the trains, the builds and the gathers) become one start and go-off pair, and the goldens may move. The plan states the target, not the contracts' shapes, and AGENTS.md asks a slice of this size for a proposal that the user reviews before it is built.
- **Options:**
  1. **A proposal first (recommended).** A document under `docs/design/` gives the contracts (`Purse`, the living test, the passive hook, the kinds' callback table), the new layer order and the golden moves it expects, and the plan's 5.3 and C-1 follow it once reviewed.
  2. **Build it from the plan's target.** The batch takes each contract's shape as it goes, and the review comes with the diff. Cost: a layer flip of this size reviewed only after it is built.
- **Blocked:** batch 5.3, and the first bullet of 5.1.

## Q8. Batch 5.5, the standing-room contract: what navigation installs for production

- **Decided (2026-10-17):** The room data in `units`.

- **Item:** 5.5, R-31, "the standing-room query is a contract that navigation installs, and `production` drops `navigation`". The `Destination` write is done: `Destination::go_to` in `units`. Two reads remain. A train's spawn place asks `Navigation::open_cell`, which reads navigation's `PathingGrid`, `RoutePlanner` and `BodyIndex` from a `&World` (`production/src/trains.rs`). A build's `Placement`, a system param, reads navigation's `NavigationBook` walls and `BodyIndex` static bodies beside its own queries (`production/src/placement.rs`).
- **Why it waits:** a contract of functions on a `&World` serves the train, but not the placement, which runs inside a system with no world of its own. The plan names no shape that serves both.
- **Options:**
  1. **The room data in `units` (recommended).** The static bodies' index and the walls become types of `units`, which navigation fills, as `Destination` is navigation's state kept there, and a `units` method answers "is this box free" and "the nearest cell a walker stands in". Cost: navigation's index and its walls move down a layer, with the pathing grid's open-cell query.
  2. **A trait of `units` that navigation implements, read as a resource of boxed functions.** The placement takes the resource beside its queries, and the train takes it from the world. Cost: a call through a box for each placement test, and a contract of two shapes.
  3. **Keep the dependency.** `production` keeps `navigation` for the two reads. Cost: R-31 stays in part.
- **Blocked:** the rest of R-31.

## Q9. Batch 5.5, `UnitOrdered`: what carries a build, a gather and a use

- **Decided (2026-10-17):** The commands stay, and the event describes them.

- **Item:** 5.5, R-20, "`UnitOrdered` is an entity event, and `orders` drops `production` and `interaction`". `orders` names production's `BuildTarget`, `Builder` and `Gatherer`, as `OrdersAction::Build` and a slot's gather set production's loops in `UnitOrder::give` (`orders/src/unit_order.rs`), and interaction's `UnitUse` and `Using`, as the check of a use reads interaction's books before the order is given (`orders/src/unit_orders.rs`).
- **Why it waits:** an event that tells production and interaction to clear their loops does not set them. The build, the gather and the use either stay orders' commands, described by a type below all three, or become their capabilities' own commands, and the two change different things.
- **Options:**
  1. **The commands stay, the event describes them (recommended).** `units` gets `UnitOrdered { entity, order }` with an order kind of its own, and `BuildTarget` moves to `units`; orders triggers it for each order it gives, production sets or clears its build and gather loops in its observer, and interaction fills or checks the use slot in its. The command protocol stays. Cost: a use whose target fails interaction's check is dropped in interaction's observer, after orders gave it, so orders' other effects of that order need the same observer order, and the goldens may move where an observer's deferred write lands later in the tick.
  2. **The commands move.** `Build` and a gather become `ProductionAction`'s, `Use` an `InteractionAction`'s, and each capability gives its own and triggers `UnitOrdered` for the rest to clear theirs. Cost: the command protocol changes for the client, the bots and the Zero Hour check, and production's commands run in `ActionsSet::Queue`, before orders', so two orders to one unit in one tick resolve in another order.
- **Blocked:** R-20.
