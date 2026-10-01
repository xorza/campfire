# Issues
- The 3v3 creep AI chooses only enemy creeps and avatars, never a structure, so creeps never attack a tower, an inhibitor or a core.
- Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. The 3v3 camp AI fails on `unit.spawn_pos`.
- `ctx.add_resource` takes any name, and no call or load check reads the mode's `resources` list; player resources are kept by their text name.
- A restore brings back units without their derived stats and tags, and the stats refresh never computes them again: a restored stunned unit acts, and a restored slowed one walks at full speed.
