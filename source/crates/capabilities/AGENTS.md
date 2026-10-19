# campfire-capabilities

The engine's capabilities and the core under them, a crate each: the core's crates in `core/`, each capability in its own folder, and `campfire-capabilities`, which installs them, in this one. The design is `docs/design/04-capabilities/` and [Modules](../../../docs/design/02-engine-core.md#modules); the repository's `AGENTS.md` comes first, with its Direction. Each convention below has a release test or a lint that fails when it breaks.

## Where a thing goes

- **A new module** is a crate on a layer above everything it depends on, its layer read from its manifest, and one entry of the release's module list, after the modules it builds on. Its crate depends on another capability exactly as its `NEEDS` and `USES` say.
- **The core names no capability.** A capability adds to the core through its seams: a column of the script view, a call part, an effect type, a row of the script API.
- **Roles by name.** A module's effect is `<Module>Effect`, its script view column `<Module>Column`, its call part `<Module>Call`, its script API `<Module>Api`, each in the file of its name.
- **Systems** are associated functions of unit structs, one struct for each concern in its own file. A capability's `lib.rs` holds its type, its declarations and install, its system sets and its published surface, and no system.
- **The published surface** of a crate is its `lib.rs`'s `pub use` list; an item another crate uses, this crate's tests among them, is `pub`, and the rest `pub(crate)`.

## Determinism

- A system that spends something shared, takes stable ids or runs scripts walks its units in stable-id order, never in query order.
- No `HashMap` or `HashSet`, and no schedule ambiguity: two systems that touch one thing are ordered against each other.

## State

- Every state type, resource, book and registry of callbacks is declared once, by the module that owns it, and declared by what it is: state, scratch clear between ticks, a cache built again, a tick's report for the host, a book, the id of a system run out of the schedule. A resource none names, or a scratch not clear after a tick, fails the proving match.
- A restore checks every value as package data is checked: a restore gives an error, never a panic.
- What can be derived is derived, not state: stats, tags, supply, the pathing grid.
- What a client receives from the server it does not compute: a system that computes the server's own state goes through `InstallCx::server`, and a part only a server derives is a server part.

## Names and scripts

- Text becomes a type where it enters. Each id's names are one table, which its book holds and the script view shares; a lookup of an id by its name happens only in the load or a script call, and each such lookup is listed for review.
- The script API is bound through its builder, which checks a call's role before its body runs; each argument is a typed script argument that resolves a name. `docs/design/08-script-api-reference.md` is generated from the registry, never edited by hand.
- No genre words in production code: a mode names its heroes, lanes and resources in its data. The engine's words are the model's: unit, tag, stat, pool, modifier, action, effect, avatar, spawn group, path, marker.

## Tests

- A test that builds a `Match` lives in this crate's `tests/`, in the module of the crate it tests, and reads that crate's privates through `internals`; a test of a crate's own types, with no `Match`, is a unit test in its crate. A test helper that needs a higher crate is a method of a fixtures trait of this crate's `internals`.
- A test that runs a match holds a check of the warnings it logs; a warning no test takes fails it.
- A change that moves a golden says why in its commit, with the number that moved. A refactor moves neither column; a layout change moves only the state column.
