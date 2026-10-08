# Questions — review-crates

## Owner-only replication needs `bevy_replicon` as a direct dependency

**Item:** `source/crates/campfire-net/src/net_protocol.rs:94,102,106,107,110`. `SpawnPoint`, `Respawn`, `Route`, `Progress` and `ModifierClocks` go to every observer, though only the owner's prediction reads them.

**Why it is blocked:** a filter that hides some components and not the whole entity is replicon's component-scope `VisibilityFilter`. Lightyear 0.30 uses it, but it re-exports neither the trait nor `AppVisibilityExt::add_visibility_filter`. Lightyear's own tools (rooms, `gain_visibility` and `lose_visibility`) act only on whole entities. So the fix needs `bevy_replicon` as a direct dependency of `campfire-net`. Your rules say a new dependency waits for your word.

**Options:**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Add `bevy_replicon = "=0.44.2"` to the workspace, used by `campfire-net` (recommended) | One immutable `OwnedBy(Option<PlayerSlot>)` on each unit, a filter scoped to the five components, visible to the link whose `PlayerLink` holds that slot. | No new code in the tree: Lightyear already builds the same version. The pin must follow Lightyear's in each upgrade. |
| B. Leave it | Every observer keeps receiving the five components. | `Progress` costs bytes each tick that a seen unit walks, and `Route` resends its whole list on each change. |
| C. Send them as messages to the owner | No dependency. | A second path for state, outside replication and its rollback history. |

**Blocked:** this item only. The rest of the replication group is done with Lightyear's rooms.

## Sharing the parsed scripts needs Rhai's `sync` feature, or a cache that stays on one thread

**Item:** `source/crates/campfire-package/src/mode_packages.rs:179-194` with `source/crates/campfire-runner/src/match_build.rs:50`. Each match parses every script again, though the load parsed them with the same engine setup. The verifier builds a match for each checkpoint.

**Why it is blocked:** the plan's target keeps each `AST` in an `Arc` on its `Script`. Without Rhai's `sync` feature, an `AST` holds `Rc`s and is not `Send`. `ModePackages` goes into resources (`Lobby`, the local server, the sim client) behind an `Arc`, so it must be `Send + Sync`. The target as written does not compile.

**Options:**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Turn on Rhai's `sync` feature (recommended, after a measurement) | `AST`, `Engine` and `Module` become `Send + Sync`. `ModePackages` keeps the parsed ASTs, and the shared API modules can cross threads too, so a match build parses and binds nothing. `ScriptHost` and `ScriptFailures` can become normal resources. | Rhai uses `Arc` and `RwLock` in place of `Rc` and `RefCell`: each call pays atomic counts. Every type a script holds must be `Send + Sync`. I would measure the 3v3 bench before and after, and keep it only if the worst tick does not get slower. |
| B. A parsed-script set that stays on its thread | The verifier, or any owner that builds many matches, parses once and gives each match host clones of the ASTs. A clone shares the functions through an `Rc`. | No feature change. Only callers that keep the set gain, and each match still binds the API. A second path through `MatchBuild`. |
| C. Leave it | Each match parses its scripts. | The verifier parses the whole mode once per checkpoint. |

**Blocked:** this item only. The other item of its group (tag names and stat order once) is done.

## The JSON log file's writer: a worker of `store`, which the design does not let `log` use

**Item:** `source/crates/campfire-log/src/logging.rs:62`. The file layer writes through an unbuffered `Mutex<File>`, one `write(2)` per event, on the thread that logs, the server's tick thread included.

**Why it is blocked:** the target is a writer that `store`'s `Worker` owns, fed by a bounded queue. The design says `log` depends on `common` alone (02-engine-core.md, Dependencies), and lists `log`'s JSON file among the allowed writers outside `store` (Storage, Enforced). So the fix changes the crate graph that the design fixes, which is your call.

**Options:**

| Option | What it does | Cost |
| --- | --- | --- |
| A. `log` depends on `store`; a `Worker` writes the lines (recommended) | Each event goes into a bounded queue of byte buffers, and a worker named `log` writes them. A full queue makes the logging thread wait, so no line is lost. The design's dependency line and the allowed-writers list change to match. | One more edge in the crate graph; `store` depends on no engine crate, so no cycle. A crash that aborts loses the lines still in the queue, at most its bound. |
| B. A `BufWriter` in the `Mutex`, flushed at each Warn or Error and every N lines | No thread, no new edge. | Writes still run on the logging thread, fewer of them; a crash that aborts loses the unflushed Info lines. |
| C. `tracing-appender`'s non-blocking writer | The established crate for this. | A new dependency, which you approve first; it drops lines when its queue is full unless set to block. |
| D. Leave it | — | One syscall per event on the tick thread. |

**Blocked:** this item only.
