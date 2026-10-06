# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Structure and names

The design is [Structure and names](docs/design/12-structure.md): one rule for each kind of value, one place for each error, one name for each meaning, and one copy of each piece of wiring. The renames, moves and splits come first, as pure refactors whose tests move with the code and pass unchanged; the shared parts follow, each with its tests. The net scenarios, the restore and checkpoint tests, and the LAN check pass at each step.

1. **R7. App wiring** (net, server, client; [U4](docs/design/12-structure.md#decisions)): `SimServer` and `SimClient` add their plugins, the replication observer, the prediction manager and the client's entity; the server, `LocalServer`, the harness and the tests add only their own; one headless frame. Tests: none new; the net scenarios and the LAN check cover the wiring.
2. **R8. Test helpers** (protocol, store, net, server, runner, capabilities, package; [Shared test helpers](docs/design/12-structure.md#shared-test-helpers)): the key of one byte in `protocol`'s `internals`, for every test; `tempfile` as a dev-dependency of `store`, `net` and `server`, and `TempDir` in every test, `ScratchDir`, `Scratch` and the hand-made directories gone; `Aim`, the tick rate and the builders in `runner`'s `scripted.rs`; `PackageContent`'s list of keys. Tests: the list of keys names exactly the fields.
