# campfire-capabilities

The engine's capabilities and the core under them, a crate each: the core's crates in `core/`, each capability in its own folder, and `campfire-capabilities`, which installs them, in this one. The design is `docs/design/04-capabilities/` and `docs/design/02-engine-core.md`, whose Modules table maps the crates; this file names the crates' conventions and the test or lint that holds each, so a change finds them before a test fails. The repository's `AGENTS.md` comes first.

## Where a thing goes

- **Layers.** A crate depends only on the crates of lower layers, and a capability needs only capabilities of lower layers: `LAYERS` in `src/release/tests/mod.rs`, `a_capability_crate_depends_only_on_lower_layers` and `each_capability_installs_after_the_lower_layers_it_builds_on`. A module implements `Module`, in `core/declare/src/declare/module.rs`: its capability, what it builds on (`NEEDS`) and what it reads when the match has it (`USES`), which together are its crate's dependencies on other capabilities (`a_capabilitys_crate_depends_on_exactly_what_it_needs_or_uses`), everything it adds but its systems, declared once, which the install, the script API, the network layer and the declaration audit read, and the install of its systems. A new module is a crate, a row of `LAYERS`, and one entry of `Release::modules`, in `src/release/mod.rs`, after the modules it builds on.
- **The core names no capability.** `units`, `scripts`, `players`, `stats`, `actions` and `values` know no capability above them; a capability adds to them through their seams: a column of the script view, a call part, an effect type, a row of the script API.
- **Roles by name.** A module's effect is `<Module>Effect` in `<module>_effect.rs`, its script view column `<Module>Column` in `<module>_column.rs`, its call part `<Module>Call` in `<module>_call.rs`, its script API `<Module>Api` in `<module>_api.rs`: `each_role_of_a_module_takes_the_modules_name`.
- **Systems.** A capability's systems are associated functions of unit structs, one struct for each concern in its own file, as `combat/src/attacks.rs` holds `Attacks`. A column's `fill_row` and its `RowReads` live with the column type. A capability's `lib.rs` holds its struct, its `install`, its system sets, its module list and its `pub use` list, and no system.
- **The published surface** of a crate is its `lib.rs`'s `pub use` list, and its modules stay private; the workspace lint `unnameable_types` keeps every type it reaches published. An item another crate uses, this crate's tests among them, is `pub`, and the rest stays `pub(crate)`. No crate re-exports another's items: a crate depends on the home crate of each type it names, `no_crate_re_exports_another_crate` in `campfire-common`.

## Determinism

- A system that spends something shared, takes stable ids or runs scripts walks its units through `Ordered`, by stable id, never in query order: the archetype-shuffle test plays the proving match in reverse query order.
- No `HashMap` or `HashSet`, and no schedule ambiguity: `clippy.toml`. Two systems that touch one thing are ordered against each other.
- Numbers in the sim are `Num`, exact, rounded once where the design says; no float.

## State

- Every state component is declared once, in its module's `declare`, which the install, the network layer and the script API read, and each module declares only its own: the declaration audit, `src/release/tests/declaration_audit.rs`, checks the rules between declarations.
- Every resource of a match is declared by what it is: state, `d.resource`, which the installer puts in at its default; scratch, `d.scratch`, clear between ticks; a cache, `d.cache`, built again from state, the map or the install; a tick's report for the host, `d.report`; or a book, `d.book`; a registry of callbacks is declared by each module that calls it, `d.calls`, and by each that gives it a callback, `d.callback`, the key's owner. `Release::unlisted_resources`, which the proving match runs after each tick, fails on a resource none names and on a scratch not clear.
- A restore checks every value as package data is checked, through the state type's required check: a restore gives an error, never a panic. The snapshot tests flip and draw the bytes of whole matches.
- What can be derived is derived, not state: stats, tags, supply, the pathing grid.

## Names and scripts

- Text becomes a type where it enters. Each id's names are one `Names` table, which its book holds and the script view shares; a lookup of an id by its name is the table's `named`, called only by the load or a script call: `LOOKUPS` in `src/release/tests/mod.rs` lists each, and a new one joins it.
- The script API is bound through `ApiBuilder`: `call` for a call or a value of `ctx`, which checks the role before the body runs, `method` for a method or an operator of a handle, and `bind` for a value or a field's getter, which takes no argument. Each argument of a call or a method is a `ScriptArg`: its type resolves a name, records the name's kind in the member's spec, and binds the integer form of a `Num`. A table of package data is a `DataSection`, which `data` records by its type. `docs/design/08-script-api-reference.md` is generated: write it again with `CAMPFIRE_BLESS=1` and the test `the_reference_is_what_the_registry_writes`.
- No genre words in production code: a mode names its heroes, lanes and resources in its data. The engine's words are the model's: unit, tag, stat, pool, modifier, action, effect, avatar, spawn group, path, marker.

## Tests

- A new rule of a mode's match joins that mode's scripted match, its event asserted in its own tick: `docs/design/02-engine-core.md`, Testing and diagnostics.
- A change that moves a golden says why in its commit, with the number that moved. A refactor moves neither column; a layout change moves only the state column.
- A test that builds a `Match` lives in this crate's `tests/`, in the module of the crate it tests, and reads that crate's privates through `internals`. A test of a crate's own types, with no `Match`, is a unit test in its crate. A test helper that needs a higher crate is a method of a fixtures trait in `src/fixtures/`, `<Module>Fixtures`, which this crate's `internals` publishes.
- A crate's `internals` turns on `internals` in each dependency that has it. A crate's dev-dependencies are itself with `internals`, and only the tools its tests use that are not its dependencies: `campfire-codec` and `campfire-store`.
- A test that runs a match holds a `LogCheck` through its fixture (here a `Match` from `Match::test`); a warning no test takes fails it.
- Benches live in `bench.rs` beside what they measure, behind the `internals` feature.
