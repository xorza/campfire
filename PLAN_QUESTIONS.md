# Plan questions

## W3. One spawn for the tests

**Item.** W3 says the tests' 32 spawns become `Units::spawn` of a `UnitCore` and the parts each capability's own constructor gives, `Armed::parts`. The rest of W3 is done: the test traits are gone, and each spawn helper is now a test function of its capability type, such as `Combat::spawn_armed`, `Abilities::caster` and `Navigation::body`.

**Why it needs a call.** One generic spawn makes most call sites longer. Most helpers add more than parts: a destination, pools, an attack target, or a default slot table inserted after the spawn. Also, `Armed::bundle(world)` needs the world, so a call site cannot build the parts inside the same call that borrows the world. An example, before and after:

```rust
let still = Abilities::spawn_combatant(&mut game, 1, ground(Num::int(10), Num::ZERO), ());
let still = Units::spawn(&mut game, Spot::at(ground(Num::int(10), Num::ZERO), Team::new(1)), Abilities::combatant(()));
```

**Options.**

| Option | What it does | Cost |
|---|---|---|
| A. Keep the capability helpers (recommended) | Each helper stays a test function of its capability type. The parameter order and the team type become the same everywhere: `(world, [unit_type,] pos, team, …)` with `Team`. | 32 helpers stay, but in their owners' namespaces. |
| B. One `Units::spawn` | One `Units::spawn(world, spot, parts)`, where `parts` is a `UnitParts` trait that `Armed`, every `Bundle` and a few capability part builders implement; the helpers go. | About 520 call sites change, and most of them become longer. |

**Blocked.** Only the spawn part of W3. W4 to W6 do not depend on it.
