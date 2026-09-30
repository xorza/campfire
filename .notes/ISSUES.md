# Issues

- Two `PlayerSlot` types exist, `campfire_protocol::PlayerSlot` and `campfire_sim::PlayerSlot`, each a `u32`; code that meets both compares them through `get()`.
