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
