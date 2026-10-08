# Review: campfire-capabilities

Whoever addresses an item deletes it. A group whose items are all gone is deleted too.

Paths are relative to `source/crates/campfire-capabilities/src/` unless they name a crate. Groups are named after the root cause their items share, and sorted by severity and benefit across the whole crate: wrong behavior first, then untrusted-data robustness, then structure, then cleanup. An item marked **(bug)** was confirmed against the code: the behavior is wrong today.

Fix the root cause of a group, not its items one by one. Most groups give the structural target first, and their items are the places that target removes.

## 10. Script call plumbing is written once per caller

The same steps to begin a frame, call a hook within the budget, apply on success and record on failure are written in each system. Also, each capability applies its effects in a different shape. Target: one `ScriptBatch` (or `Calls`) method that runs a hook and does all of those steps, and one shape for `Effect::apply`.

- [ ] `combat/mod.rs` (8 free-function systems, `Wielded`, `GoingOff`, `IntervalDue`, the `RowParts` and `Attacker` aliases, about 620 lines) and `stats/mod.rs:263-302` (`expire_modifiers`, `clear_dead_modifiers`, `fill_row`), while `DamagePass`, `Refresh` and `HeldPass` are unit-struct namespaces in their own files. Two conventions for one job. Target: one type per system, in its own file. `mod.rs` keeps `install`, the sets and the module list. **Decided: crate-wide. Every capability's systems move into unit-struct files by concern, and each `fill_row` becomes a method of its column type; done after the other items, one capability per commit.**
