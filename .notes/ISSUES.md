# Issues
- The 3v3 creep AI chooses only enemy creeps and avatars, never a structure, so creeps never attack a tower, an inhibitor or a core.
- Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. The 3v3 camp AI fails on `unit.spawn_pos`.
- An avatar package's `orders` fails the load with "no data names it" for its AI script: the load check names the AI scripts of the mode's unit types alone, while the match build loads an avatar's `orders`.
