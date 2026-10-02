# Issues
- Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. Rime's Snow Owl fails on `ctx.reveal` as it ends.
- The client draws no area units: a unit with an `Area` is left out of the drawn units, and nothing shows where an area lies or how far it reaches.
- Cinder's `chain_fire.rhai` writes `next.state` on the result of `ctx.projectile`, which returns `()`: every bounce of Chain Fire that finds a next target fails its `on_hit` call.
- A map cannot block cells: design 04's navigation gives `[navigation]` the cells its map blocks on each layer, but the map data and `PathingGrid` hold none. The exact tests of a route, smoothing and the straight-goal shortcut, know only bodies.
- A script that makes a function pointer, a closure, an anonymous function or a call of `Fn`, loads, though design 08's checks refuse it; its `.call` fails the load as a member no handle has, not as a function pointer.
