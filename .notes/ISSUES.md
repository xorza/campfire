# Issues
- The 3v3 creep AI chooses only enemy creeps and avatars, never a structure, so creeps never attack a tower, an inhibitor or a core.
- Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, a hook never runs, and a state or a data field changes nothing. The 3v3 camp AI fails on `unit.spawn_pos`.
