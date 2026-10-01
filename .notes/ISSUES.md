# Issues
- Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. Rime's Snow Owl fails on `ctx.reveal` as it ends.
- `mode_inputs`, an exclusive system in Inputs, has no stated order against `expire_modifiers` and `track_static_bodies`, also in Inputs. The schedule's ambiguity check does not report the two pairs, and reports them once another Inputs system ordered after `CombatSet::Respawn` uses deferred commands.
