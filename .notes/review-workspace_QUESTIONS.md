# Questions of review-workspace

## Batch 8: the books and engine tags from the declaration walk

The item: "`capabilities/src/books/{book_builder.rs:229-321,book_parts.rs,unit_type_data.rs}`, `areas/src/lib.rs:115`, `projectiles/src/lib.rs:113`, `production/src/lib.rs:188-202`: each module's book declaration carries its build function, `into_books` walks the declarations, and the engine tags are numbered by the walk."

Blocked: this item only. The other Batch 8 items do not need it.

### Q1. The books' build

Now `into_books` adds each of 45 books to `BuiltBooks` by hand. `BookFill` walks the declarations and takes each declared book, and it panics on a declared book that is not built. The books test fails on a built book that no module declares. So the two lists cannot disagree in a match, but a new book needs a line in each.

A build function on each declaration cannot read `BookParts`: it is in the top crate, which names every capability. The data that each book comes from (the unit type sections in `UnitTypeData`, the names, the stat resolution) is also parsed and resolved by the builder.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. Keep the list (recommended)** | `into_books` keeps its list, which the declared-and-built check and the books test hold. | A new book is two lines, in two places, each checked. |
| B. A modular load | Each module declares its unit type section and its book's build through a build context in `declare`. The builder walks the declarations, and `UnitTypeData` becomes the sections that the modules declare. | A redesign of the load: `BookBuilder` (1,174 lines), `UnitTypeData` and the package load check. It needs its own design in `docs/design/` and its own step. |

### Q2. The engine tags' numbers

Now each engine tag is a `const` with its number (`Areas::TAG = EngineTag::new(3, …)`). `Release::engine_tags` walks the declarations and asserts that each number is the tag's place in the walk, at each match load.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. Keep the const numbers (recommended)** | The number stays in the `const`, and the walk checks it. | A new tag's author writes its number, and the walk refuses a wrong one at once. |
| B. Numbers from the walk | A tag's number is its place in the walk, held in a resource. | Each `TAG` stops being a const value: its 27 uses (`Dead::TAG.tag()`, `StatusTags::from([Production::CONSTRUCTING])`, the filters on hot paths) read the number from the world. |

## Batch 8: the vision group of a new unit

The item: "`values/src/relations.rs:94-100`: `seen_targets.rs:39` reads `VisionGroups`."

Blocked: this item only.

`SeenTargets::sees` and `VisionColumn::seen_by` call `Relations::vision_group` for a unit that spawned after the last Vision stage. The call derives the group from the current relations, at most 256 steps, and only for such a unit. `VisionGroups` is a `Local` of the Vision pass, in its `Fog`, and is rebuilt only in that pass, when the relations changed.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. Keep the derivation (recommended)** | Both readers derive the group from the relations they read, so it is never stale. `Relations::vision_lowest` stays the one derivation that both `vision_group` and `VisionGroups` use. | A derivation of at most 256 steps for each check of a new unit. |
| B. A shared `VisionGroups` | `VisionGroups` becomes a resource, rebuilt as the relations change, and the readers read it. | A mode script changes the relations in Mode, and an order check or a view refill that runs before the rebuild reads stale groups. A stale group shows a new unit to a team that no longer shares vision. Each reader then needs a rebuild ordered before it. |

## Batch 9: the protocol's signer

The item: "Protocol signing: a `Signer { secp, key }` and a verification context with its scratch message, used by `checkpoint`, `session_result`, `server_input`, `receipt`, `input_chain`, `connect` and `session_log`. `ServerSigner` goes."

Blocked: this item only.

Each signature takes BIP-340's auxiliary randomness. Now `net`'s `ServerSigner` holds the key, the signing context and `net`'s `Entropy`, and fills the randomness for each signature. A `Signer { secp, key }` in protocol has no randomness, and protocol has no `bevy`, so it cannot be a resource. The verification context with its scratch message has no open decision.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. The session signs (recommended)** | `Entropy` moves to `common`. Protocol's `Signer` holds the key, the context and the entropy. The runner's `Session`, a resource, holds the server's `Signer` and signs its own inputs, checkpoints and result, and `ServerSigner` goes. The client's `Signer` becomes protocol's too. | `Session::serve` and the checkpoint and result paths change in `runner` and `net`. A replay or a verifier holds no signer, so the session's signer is an `Option` that a server alone fills. |
| B. A plain `Signer`, and `net` fills the randomness | Protocol's `Signer { secp, key }` replaces each `(secp, key)` pair. `ServerSigner` stays, as the resource that pairs the `Signer` with `net`'s `Entropy`. | The item's "`ServerSigner` goes" is not met: a wrapper stays. |
| C. Fixed auxiliary randomness | The signatures take no fresh randomness, as BIP-340 permits, and `ServerSigner` goes. | BIP-340 recommends fresh randomness against side channels. Every pinned signature moves. |

## Batch 10: the units file's writer

The item: "`import/src/zero_hour/map_import.rs:69`, `unit_types.rs:20`: `MapData`, `PlacedUnitData`, `MarkerData` and the units file type derive `Serialize`, and the import writes them." The map part is done.

Blocked: the units file part of this item.

The units file type is `package`'s `UnitsFile`, a map of `capabilities`' `UnitTypeData`. `UnitTypeData` holds a section of each capability, so a `Serialize` on it needs one on about forty data types in a dozen crates, several with readers of their own that refuse values, each of which must write what it reads. The import writes only empty unit types, `[units.<name>]`, through a small type of its own.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. Keep the import's writer, and test it (recommended)** | The import keeps its `UnitsToml`, and a test reads what it writes through `package`'s `UnitsFile`, so a change of the format fails the test. | Two types say the units file's shape, one of them the import's, for the names alone. |
| B. `Serialize` on every unit type section | `UnitTypeData` and each capability's section derive or implement `Serialize`, each with a round-trip test, and the import writes `UnitsFile`. | About forty types in a dozen crates; the import depends on `package` and through it on every capability. |

## Batch 11: change-driven passes, now or in an optimization session

Every item of Batch 11 changes what a pass costs and keeps what it gives: the `DueRun`, the static bodies' patches, the routing's and the steering's changed sets, the held modifiers, the passives, the gathers and the homing shots, the seeds' derivation, the admission map, and the box probes. Its last item already waits for the user's optimization session, to run each item's bench.

AGENTS.md: "Optimization has its own sessions, which the user starts; until then a step's checks are its tests." The established practice is to measure a pass before and after a change of its cost. Some of the changes add work per change (a sorted run kept in step, change filters, a dormant set for dead thinkers) to remove work per tick, which pays only where the bench shows the scan's cost.

Blocked: all of Batch 11.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. In the optimization session (recommended)** | The batch moves whole to the user's next optimization session, where each item's bench measures the pass before and after its change, and an item that does not pay is dropped. | The scans stay until then. |
| B. Now, measured later | Each item is written now, its tests holding its results, and the session measures them. | Changes land unmeasured; a change that costs more than the scan it removes stays until the session finds it. |

## Batch 13: the `JoinRefused` event's fields

The item: "`net/src/events/{save,receipt,join}_refused`: typed fields, as `LinkLost` has." `SaveRefused` and `ReceiptRefused` are done: each carries its refusal enum, which serializes as `LinkLost`'s cause does.

Blocked: `JoinRefused`.

`JoinRefused` holds `link`, the link's entity as `Debug` writes it, and `error`, the `JoinError`'s report as text. `JoinError` wraps `DelegationError`, whose `NotEvent` case holds nostr's error, which has no serde form, and `ServerInputRefused` from `runner`. The other lines of the lobby name a link by `Debug` too, as `link = ?entity`.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. A serializable join refusal (recommended)** | `JoinError` and the errors it wraps derive `Serialize` and `Deserialize`. `EventError` keeps nostr's error kind, as a `Copy` enum of its own, in place of the `Arc` of nostr's error, which its `PartialEq` already compares alone. `JoinRefused` carries the `JoinError`, and the link as its entity's bits; the lobby's other lines name links the same way. | `EventError` keeps no source chain beyond the kind. `runner`'s `ServerInputRefused` and `protocol`'s `ScopeError` gain serde derives. |
| B. A kind beside the text | `JoinRefused` gains `kind`, a field-less enum of `JoinError`'s cases that `JoinError::kind` gives, and keeps the report as text, with the link as now. | Two enums say the cases of a join refusal; the compiler keeps the match exhaustive. |
| C. Keep the text | The report is human text, which the rule lets a string hold; the link stays as the text the lobby's lines use. | A test or a reader of the log matches a refusal by its text. |

## Batch 14: one case of `ApiError` for a capability's own refusal

The item: "`script/src/scripts/error.rs:26`: one case that carries a capability's own refusal type. The validators move to the argument code." The validators are done: a radius is a `ScriptRadius` argument, and an integer's check is the `Num` argument's own.

Blocked: the one case.

`ApiError` is one flat enum of 70 cases, `Copy` and `PartialEq`, in `script`, below every capability. About 45 of its cases are one capability's: the choices' and the state's are the mode's, `NoTrack` and `NegativeXp` progression's, `NoAttack` combat's, `NoWalker` and `NotASpeed` navigation's. It has 208 uses, 94 of them in tests, which compare a failure as `FailureKind::Api(ApiError::…)`. `script` cannot name a capability's type, so the one case needs a way to carry a type from above it.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. A refusal code by capability (recommended)** | `ApiError::Refused(Refusal)`, with `Refusal { owner: Capability, code: u8 }`, `Copy` and `PartialEq` as now. Each capability's refusal enum converts into it and back by `From` and `TryFrom`, and gives its message through a table its module declares, as the callbacks are. A test compares `ApiError::from(CombatRefusal::NoAttack)`. | About 45 cases move to a dozen enums, and the 208 uses change. A message is found by a lookup, not by `thiserror` on one enum. |
| B. A static refusal of a trait object | `ApiError::Refused(&'static dyn CapabilityRefusal)`, each case a `static` of its capability, compared by its name. | `dyn` where an enum serves; equality by name. |
| C. Keep the flat enum | `ApiError` stays the script API's one list of refusals, which the reference documents together. | The cases of a capability stay in `script`, and a new capability's refusal changes `script`. |

## Batch 14: the guide items left unchanged

The item: "Small guide items, also in Section 3." Each guide item is done but the five below. One waits for the Batch 9 question. For the other four, I recommend no change, and the plan does not keep them.

| Guide item | Why it stays | Option to change it |
| --- | --- | --- |
| `net/src/sim_client/mod.rs`, `lose_link` and `retry_link` take `Res<Signer>` only for the retry jitter's random bytes | Blocked by Batch 9: the home of `Entropy` (`common`, or `net` as now) decides what these systems take. The plan keeps this item. | After Batch 9, the systems take the `Entropy` that the client holds. |
| `capabilities/src/books/book_builder.rs:558`, the `mem::take` of `books.effects` | `BookBuilder::names` borrows the whole builder, and `effects.push` needs `books.effects` mutably at the same time. The swap is three lines, and it puts the book back before the error returns. | A struct of the name books apart from `effects` in `BookParts`, so the two borrows are of different fields. This moves about ten fields and their uses. |
| `mode/src/mode_map.rs:162`, `ModeMap::install_pathing` | The review saw one caller. Now `Mode::load` and `Books::install_prediction` both call it, so the wrapper keeps the map's pathing in one place. | Inline it into both callers. |
| `import/src/zero_hour/model_writer.rs:107`, `model_writer::path` | It is a free function with no type to own it, which the guide permits: the path is `campfire_package`'s `PackagePath`, and it takes no writer. | An associated function of `ModelWriter` that does not construct, which the guide does not permit. |
| `net/src/sim_client/mod.rs:640`, the clone of each due `ScriptedInput` | The clone occurs once for each mode input of a script in a match, not for each tick. The pending input can stay in the queue after the tick, so it cannot borrow the script. | `Pending::Input` holds the input's place in `BotScript`, and the send reads the script again. The bot script must then stay while an input waits. |
