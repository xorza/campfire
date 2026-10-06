# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Benches

The design is [Structure and names](docs/design/12-structure.md), U13: one bench target for each crate, named for its module, whose cases criterion's filter chooses by their `<group>/<case>` id; CI runs each case once.

1. **B1. One target for each crate** (math, capabilities, net, protocol, runner, sim; [U13](docs/design/12-structure.md#decisions)): each crate's `lib.rs` `bench` facade gives `run`, which runs each of its bench functions; each crate has one `[[bench]]`, `<module>`, in `benches/<module>.rs`: `math`'s `num`, `rng` and `vec3` targets become `math`, and the other five take their crate's module name. `protocol`'s cases join the `chain_head` group, as `sign` and `check`. The navigation chapter's command becomes `cargo bench -p campfire-capabilities --features bench -- collision/`. Check: each crate's `-- --list` lists its ids, and `-- --test` runs each case once.
2. **B2. A heavy fixture in the first case that needs it** (runner, net; [U13](docs/design/12-structure.md#decisions)): the runner's `Reference3v3` in a `LazyCell`, and its `mean` case's match in an `Option`; each of `net`'s two rollback matches in an `Option` of its case. Check: `-- --list` for the runner and for `net` returns at once, and `-- --test` runs each case once.
3. **B3. Each case once in CI** (CI; [U13](docs/design/12-structure.md#decisions)): a step `Benches` after `Test`, `cargo test --workspace --benches --all-features --locked`, on every platform. Check: the step passes on the three platforms, and a case made to panic fails it.
