# campfire-capabilities

The engine's capabilities and the core under them. The design is `docs/design/04-capabilities/` and `docs/design/02-engine-core.md`; this file names the crate's conventions and the test or lint that holds each, so a change finds them before a test fails. The repository's `AGENTS.md` comes first.

## Where a thing goes

- **Layers.** A module imports only from its layer and below: `LAYERS` in `capability_set/tests.rs`, `a_module_imports_only_from_its_layer_and_below`. A capability's install order and what it builds on are `CAPABILITIES` in `capability_set/mod.rs`. A new module gets a row in both.
- **The core names no capability.** `units`, `scripts`, `players`, `stats`, `actions` and `values` know no capability above them; a capability adds to them through their seams: a column of the script view, a call part, an effect type, a row of the script API.
- **Roles by name.** A module's effect is `<Module>Effect` in `<module>_effect.rs`, its script view column `<Module>Column` in `<module>_column.rs`, its call part `<Module>Call` in `<module>_call.rs`, its script API `<Module>Api` in `<module>_api.rs`: `each_role_of_a_module_takes_the_modules_name`.
- **Systems.** A capability's systems are associated functions of unit structs, one struct for each concern in its own file, as `combat/attacks.rs` holds `Attacks`. A column's `fill_row` and its `RowParts` live with the column type. `mod.rs` holds the capability's struct, its `install`, its system sets and its module list, and no system.
- **The published surface** is `lib.rs`'s `pub use` list; `#![warn(unnameable_types)]` keeps every type it reaches published.

## Determinism

- A system that spends something shared, takes stable ids or runs scripts walks its units through `Ordered`, by stable id, never in query order: the archetype-shuffle test plays the proving match in reverse query order.
- No `HashMap` or `HashSet`, and no schedule ambiguity: `clippy.toml`. Two systems that touch one thing are ordered against each other.
- Numbers in the sim are `Num`, exact, rounded once where the design says; no float.

## State

- Every state component is registered at install (`registry.register_component`), and each capability adds only its own: `each_capability_adds_exactly_its_own_state_types`.
- A restore checks every value as package data is checked, through the state type's required check: a restore gives an error, never a panic. The snapshot tests flip and draw the bytes of whole matches.
- What can be derived is derived, not state: stats, tags, supply, the pathing grid.

## Names and scripts

- Text becomes a type where it enters. A lookup of an id by its name is a method whose name ends in `named`, called only by the load or a script call: `LOOKUPS` in `capability_set/tests.rs` lists each, and a new one joins it.
- The script API is bound through `ApiBuilder`: `bind` for a member that serves every role, `bind_for` for one that serves some roles, which checks the role before the body runs. `docs/design/08-script-api-reference.md` is generated: write it again with `CAMPFIRE_BLESS=1` and the test `the_reference_is_what_the_registry_writes`.
- No genre words in production code: a mode names its heroes, lanes and resources in its data. The engine's words are the model's: unit, tag, stat, pool, modifier, action, effect, avatar, spawn group, path, marker.

## Tests

- A new rule of a mode's match joins that mode's scripted match, its event asserted in its own tick: `docs/design/02-engine-core.md`, Testing and diagnostics.
- A change that moves a golden says why in its commit, with the number that moved. A refactor moves neither column; a layout change moves only the state column.
- A test that runs a match holds a `LogCheck` through its fixture (`TestMatch` here); a warning no test takes fails it.
- Benches live in `bench.rs` beside what they measure, behind the `bench` feature.
