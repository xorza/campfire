# Issues
- Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. Rime's Snow Owl fails on `ctx.reveal` as it ends.
- `mode_inputs`, an exclusive system in Inputs, has no stated order against `expire_modifiers` and `track_static_bodies`, also in Inputs. The schedule's ambiguity check does not report the two pairs, and reports them once another Inputs system ordered after `CombatSet::Respawn` uses deferred commands.
- The client draws no area units: a unit with an `Area` is left out of the drawn units, and nothing shows where an area lies or how far it reaches.
- `ModePackages::stat_graph` gives the mode's modifiers no applying abilities, so a live stat change of a mode modifier that reads a param only a mode action declares adds no edge to the graph, and a loop through it is not found at load.
- `ctx.attack_hit` (`capabilities/src/combat/combat_api.rs`) and the `unit.attack_range` field treat a unit whose first weapon has a global range as a unit with no attack: both read `attack_range`, which a global range leaves empty, and fail with `NoAttack`.
