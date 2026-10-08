# Review — every crate except `campfire-capabilities`

Whoever addresses an item deletes it. Items whose fix lives in `campfire-capabilities` stay where they answer what an undeclared or unused capability costs; each says so.

Five root causes hold most items. Each group's first paragraph gives the design that removes the whole class; its items are the places that change.

## A match build repeats the load's work [low]

- [ ] source/crates/campfire-package/src/mode_packages.rs:179-194 with source/crates/campfire-runner/src/match_build.rs:50 — each match parses every script again, though the load parsed them (mode_packages.rs:385-386) with the same engine setup. The verifier builds a match for each checkpoint (`campfire-verifier/src/replay/mod.rs:78`). Target: the load keeps each `AST` in an `Arc` on its `Script`, and each match's host shares it. Together with the shared API modules, a match build parses nothing and binds nothing. Decided: turn on Rhai's `sync` feature; measure the 3v3 bench before and after, and keep it only if the worst tick is no slower, else report back. Measured, pinned to one core, and not kept: `server_tick/worst_3v3` medians 526 µs without, 530 µs with, over three runs each, about 1% slower; `mean_3v3` 1.0% slower (p < 0.05); `first_3v3` unchanged. The conversion (the script view, frame, mode, handle and spawner on `Arc` with `RwLock`, `Mutex` and `OnceLock`, and `Send + Sync` bounds on the view's columns, the frame's parts, effects and bound functions) compiles and is kept as `.notes/rhai-sync.patch`, not applied. Waits for the owner's next call.
