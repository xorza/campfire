# Questions: review-campfire-capabilities

## Q1. One convention for a capability's systems

**Item.** Group 10: "`combat/mod.rs` (8 free-function systems, …) and `stats/mod.rs:263-302` …, while `DamagePass`, `Refresh` and `HeldPass` are unit-struct namespaces in their own files. … Target: one type per system, in its own file."

**Why it needs a call.** The two conventions are not limited to combat and stats. The crate has 70 private free-function systems in the `mod.rs` of 15 modules (abilities 14, navigation 11, orders 10, combat 9, mode 5, …), and every capability that fills a script view row has a free `fill_row` with a `RowParts` alias in its `mod.rs` (actions, combat, navigation, production, progression, stats, vision). Only `DamagePass`, `Refresh` and `HeldPass` are unit-struct namespaces. The item's target changes two modules and leaves the crate with three styles. A crate-wide rule reaches every capability.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. Crate-wide: one type per system concern, in its own file (recommended)** | Each capability's systems move out of `mod.rs` into unit-struct files by concern (for example `combat/attacks.rs` with `Attacks`: start, events, pay, strike; `combat/death_pass.rs`: die, despawn, respawn). Each column's `fill_row` becomes an associated function of its column type, in the column's file. `mod.rs` keeps `install`, the sets and the module list. | A large move across 15 modules. No behaviour change. |
| B. Only combat and stats, as the item says | The two named modules move; the other 13 stay free functions. | A third mixed state; the item's "two conventions" stays true crate-wide. |
| C. Free functions stay the rule | Private free-function systems stay in `mod.rs` (the coding guide allows private free functions). Only `mod.rs` files past a size limit split their systems into submodules of free functions. `DamagePass`, `Refresh` and `HeldPass` stay, as namespaces for many private helpers. | The item is dropped; `combat/mod.rs` (905 lines) needs the size rule to split. |

**Recommendation.** A, done as its own plan step after the review items, one capability per commit.

**Blocked.** This item only. Group 15 (file layout) touches `mod.rs` files too, but its items do not depend on this choice.
