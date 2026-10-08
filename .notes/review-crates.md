# Review — every crate except `campfire-capabilities`

Whoever addresses an item deletes it. Items whose fix lives in `campfire-capabilities` stay where they answer what an undeclared or unused capability costs; each says so.

Five root causes hold most items. Each group's first paragraph gives the design that removes the whole class; its items are the places that change.

## Owner-only state replicates to every observer [medium]

- [ ] source/crates/campfire-net/src/net_protocol.rs:94,102,106,107,110 — `SpawnPoint`, `Respawn`, `Route`, `Progress` and `ModifierClocks` replicate to every observer, though only the owner's prediction reads them; `Progress` changes every tick a unit walks, and `Route` resends its whole `Vec` on change. Target: an immutable `OwnedBy(Option<PlayerSlot>)` on each unit, a replicon `VisibilityFilter` scoped to these five components, with `PlayerLink` as its client component. Blocked: see `review-crates_QUESTIONS.md`, "Owner-only replication needs `bevy_replicon` as a direct dependency".

## A match build repeats the load's work [low]

- [ ] source/crates/campfire-package/src/mode_packages.rs:179-194 with source/crates/campfire-runner/src/match_build.rs:50 — each match parses every script again, though the load parsed them (mode_packages.rs:385-386) with the same engine setup. The verifier builds a match for each checkpoint (`campfire-verifier/src/replay/mod.rs:78`). Target: the load keeps each `AST` in an `Arc` on its `Script`, and each match's host shares it. Together with the shared API modules, a match build parses nothing and binds nothing. Blocked: see `review-crates_QUESTIONS.md`, "Sharing the parsed scripts needs Rhai's `sync` feature, or a cache that stays on one thread".

## The JSON log file writes on the thread that logs [low]

- [ ] source/crates/campfire-log/src/logging.rs:62 — the file layer writes through an unbuffered `Mutex<File>`, with one `write(2)` per event on the thread that logs, the server tick included. The design lets this file stay outside the workers (a Decide item). Target: a writer that store's `Worker` owns, fed by a bounded queue. `tracing-appender` would also do it, but it is a new dependency to propose. Blocked: see `review-crates_QUESTIONS.md`, "The JSON log file's writer: a worker of `store`, which the design does not let `log` use".
