# Issues

- Two `PlayerSlot` types exist, `campfire_protocol::PlayerSlot` and `campfire_sim::PlayerSlot`, each a `u32`; code that meets both compares them through `get()`.
- `a_3v3_match_replays_to_the_same_hashes` in `crates/runner/tests/match_3v3.rs` runs 1.4 s to 2 s in a debug build, over the 1 s limit for one test.
