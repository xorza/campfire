# Issues
- Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. Rime's Snow Owl fails on `ctx.reveal` as it ends.
- `mode_inputs`, an exclusive system in Inputs, has no stated order against `expire_modifiers` and `track_static_bodies`, also in Inputs. The schedule's ambiguity check does not report the two pairs, and reports them once another Inputs system ordered after `CombatSet::Respawn` uses deferred commands.
- The client draws no area units: a unit with an `Area` is left out of the drawn units, and nothing shows where an area lies or how far it reaches.
- Cinder's `chain_fire.rhai` reads `hit.delivery.state` and writes `next.state`, but a `Unit` handle has no `state` member and `ctx.projectile` returns `()`: every bounce of Chain Fire fails its `on_hit` call.
- A map cannot block cells: design 04's navigation gives `[navigation]` the cells its map blocks on each layer, but the map data and `PathingGrid` hold none. The exact tests of a route, smoothing and the straight-goal shortcut, know only bodies.
- The LAN check failed once on the macOS CI runner and passed on the next run of the same code: "the input of slot 0 stamped 50 took effect in tick 56" (run 37023491620).
- `campfire-runner/src/runner/bench.rs` imports `Tick` at the end of the file, after its code.
