# Review: campfire-capabilities

Whoever addresses an item deletes it. A group whose items are all gone is deleted too.

Paths are relative to `source/crates/campfire-capabilities/src/` unless they name a crate. Groups are named after the root cause their items share, and sorted by severity and benefit across the whole crate: wrong behavior first, then untrusted-data robustness, then structure, then cleanup. An item marked **(bug)** was confirmed against the code: the behavior is wrong today.

Fix the root cause of a group, not its items one by one. Most groups give the structural target first, and their items are the places that target removes.
