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
