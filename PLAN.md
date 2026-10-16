# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## The dependency graph and the test code

What the audit of the crates' dependencies found, and the design that design 02 now states: [Modules](docs/design/02-engine-core.md#modules), [The capabilities' crates](docs/design/02-engine-core.md#the-capabilities-crates) (Tests, Dependencies), [Structural rules](docs/design/02-engine-core.md#structural-rules), [Testing and diagnostics](docs/design/02-engine-core.md#testing-and-diagnostics) and [The driver](docs/design/02-engine-core.md#the-driver). A reach-in that needs a private item, or a call only a proxy makes, stays behind `internals` in its crate: `store`'s scratch and pinned files, `codec`'s `TomlDocument`, the ids' constructors, the state registry's fuzz hooks. The match fixtures stay extension traits in `campfire-capabilities`' `internals`, as the release's snapshot audit calls `Units::load_test_modifiers`. Each batch ends with `./drive.py check` passing and moves no golden.

### 1. Manifests

`capabilities`, `package-disk`: their manifests. Before every other batch, as each later one reads a graph these fix.

- [ ] crates/capabilities/Cargo.toml:18, 50 — `campfire-geometry` is an optional dependency that only the integration tests in `tests/` name, and no code in `src/`; cargo's `unused_dependencies` counts those tests as its use. → A dev-dependency with `internals`, and out of the `internals` feature. Builds: the dependency rule of design 02.
- [ ] crates/session/package-disk/Cargo.toml:8-16 — `internals` lists `campfire-codec/internals` twice, and `dep:campfire-codec`, which the first entry already turns on. → One entry.

### 2. The rules crate

`common`: `tests.rs`, `lib.rs`, its manifest; `log`: `error_report/tests.rs`, its manifest; a new crate `crates/test/rules`; the root manifest; `drive.py`. After batch 1. Builds: a structural rule is a test, in one place.

- [ ] crates/base/common/src/tests.rs:1-1877, lib.rs:64-65 — the workspace's structural rules are `common`'s unit tests: the lowest crate's test build pulls in `store` and `codec`, and every change reruns `common`. → A crate `campfire-rules` in `crates/test/rules`, all test code with no `internals` of its own, which depends on `store` with its `internals` for `SourceFiles`, takes each test of the rules whole, and keeps its name; `common` keeps only the tests of its own types, and drops each dev-dependency that only the rules read.
- [ ] crates/base/log/src/error_report/tests.rs:151-228 — `every_error_the_workspace_writes_goes_through_its_report` reads every crate's sources from `log`'s tests. → Into `campfire-rules`; `log` keeps `a_report_writes_each_error_down_to_the_root`, and drops `store`'s `SourceFiles`.
- [ ] source/drive.py:76 — `WORKSPACE_READERS` is `campfire-common` and `campfire-log`. → `campfire-rules` alone, with design 02's "What a check runs again".
- [ ] new: `an_optional_dependency_is_named_by_internals_code` — no test holds design 02's rule that an optional dependency is one only code behind `internals` uses. → A test of `campfire-rules` beside `a_required_dependency_is_named_by_production_code`: each optional dependency of a crate is named in its `src/` by code that `internals`, or the feature that turns the dependency on, gates; with its own test of what breaks it, a dependency only `tests/` names among them.
- [ ] source/Cargo.toml:2-25 — the workspace has no `default-members`, so a bare `cargo build` or `cargo clippy` at the root takes `harness` and `zero-hour-check`, whose normal dependencies turn `internals` on in every crate. → `default-members` lists each member but those that unify `internals`; a test of `campfire-rules` holds it equal to the members whose normal dependencies reach a crate's `internals`, as the driver's `unifies_internals` computes them.

### 3. The test packages

`package-disk`: `harness/`, `lib.rs`, `package_dir/tests.rs`, its manifest; a new crate `crates/test/test-packages`; `runner`, `net`, `server`, `harness`: their tests' imports and manifests. After batch 2, whose default-members test then takes the new crate.

- [ ] crates/session/package-disk/src/harness/package_tree.rs, test_mode.rs, lib.rs:18-22 — the workspace's test packages are `package-disk`'s, behind `internals`, and pull `codec` into it as an optional dependency; `runner`, `net` and `server` dev-depend on `package-disk` only to reach them. → A crate `campfire-test-packages` in `crates/test/test-packages`, on `package-disk`'s surface, with `TestMode` and `PackageTree`; `TreeDir` stays behind `package-disk`'s `internals`, as its own unit tests read it, and a split of it would give those tests a second copy of the crate. `package-disk` takes `codec` as a dev-dependency; `runner`, `net`, `server` and `harness` take `campfire-test-packages`.

### 4. The LAN check's build

`drive.py`: `lan_build_step`. Independent of batches 2 and 3.

- [ ] source/drive.py:340-351 — `lan build` builds the binaries the LAN check runs in the tests' build, `--workspace --all-features`, so their `internals` gates are on: `Entropy::Fixed`, `Os::Manual`, `MatchSpec`'s flags, `Release::install`'s branches, about 25 in production code. → One `cargo build` of `campfire-server`, `campfire-client`, `campfire-verifier` and `campfire-lan-check`, each by `-p`, with no feature beyond those they ship with; the comment and design 02's LAN check say what it builds, and that the server and the verifier still carry the client's Bevy features.

### 5. Enum names

`common`: `enum_all.rs`, `named_enum.rs`; `values`: `block`, `relation`, `relation_set`, `stat`; `visibility`: `spectator_fog`, `out_view`; `navigation`: `path_walker`. Independent of the others. Builds: what can be forgotten is a type.

- [ ] crates/base/common/src/named_enum.rs:5-10; values/src/block.rs:22, relation.rs:32, relation_set.rs:23, stat.rs:35; visibility/src/spectator_fog.rs:18, out_view.rs:20; navigation/src/path_walker.rs:46 — each `NamedEnum` names its members in a table by position, so a reorder of the enum without the table reads data as another member, and the round-trip tests, which read through the same table, pass. → `enum_all!` takes each member's data name beside it, `Move => "move"`, and implements `NamedEnum` itself; the eight tables go.
- [ ] values/src/relation.rs:12, navigation/src/path_walker.rs:34 — `Relation` and `PathEnd` read data through serde's `rename_all` as well, a second source of the names `NamedEnum` holds. → Each reads data through `NamedEnum::deserialize_named`, as `Block` does.
