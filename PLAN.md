# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Storage and workers

The design is [Storage and workers](docs/design/11-storage.md): one crate, `store`, for durable files, data directories and worker threads; one fault surface; and a session log that does no IO. Each step changes behaviour only where it names a change: the net scenarios, the restore and checkpoint tests, and the LAN check pass unchanged at each.

1. **T5. The rules** (all crates, design 02; [Structural rules](docs/design/11-storage.md#structural-rules)): the two rules as `disallowed-methods` in `clippy.toml`, each call they allow with its `#[expect]` and its reason, and their rows in design 02's table of structural rules. Tests: beside `common`'s manifest test, one that `clippy.toml` lists each path of the two rules, and that no source allows the lint for a crate or a module.
