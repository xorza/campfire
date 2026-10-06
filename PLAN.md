# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Storage and workers

The design is [Storage and workers](docs/design/11-storage.md): one crate, `store`, for durable files, data directories and worker threads; one fault surface; and a session log that does no IO. Each step changes behaviour only where it names a change: the net scenarios, the restore and checkpoint tests, and the LAN check pass unchanged at each.

1. **T2. The other workers** (store, net, client; [S4, S7](docs/design/11-storage.md#decisions)): `Exchange` and `LatestWriter`, and the checkpoint thread, the receipt writer and the local server's thread on `Worker`; a handle dropped while its thread unwinds a panic does not join; `SlowSync`. Tests: a handle dropped in a panic returns while its worker waits on a sync that does not return; a sync held past a second gives `SlowSync`.
2. **T3. Data directories** (store, net, server, client, lan-check; [S5](docs/design/11-storage.md#decisions)): `DataDir`, made only by taking its lock; the server's and the client's layouts, which give every path; the client's data directory locked. Tests: each path as Stage 6 names it; a data directory from Stage 6 restores; a second client on one data directory is refused.
3. **T4. Faults** (net, server, client; [S6](docs/design/11-storage.md#decisions)): `Faults` and its table of policies, which `ServerExit` and the client read in place of each worker's own report. Tests: a failed snapshot write and a failed receipt write each reach `Faults` with their source, and the server exits as the policy says.
4. **T5. The rules** (all crates, design 02; [Structural rules](docs/design/11-storage.md#structural-rules)): the two rules as `disallowed-methods` in `clippy.toml`, each call they allow with its `#[expect]` and its reason, and their rows in design 02's table of structural rules. Tests: beside `common`'s manifest test, one that `clippy.toml` lists each path of the two rules, and that no source allows the lint for a crate or a module.
