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

## CI: the QUIC allocation audits off Linux

**Item.** The skill's finish needs a green CI run. The last run, 38198753288, passes on Ubuntu and fails on macOS and Windows in two `campfire-quic` allocation audits. Master fails the same way in each run since the audits came (38192560130, 38189278645, 38186787235), so this branch did not cause it. PLAN_NET.md holds it as R5.

**Why it needs a call.** The fix is in the QUIC transport, which the `net` worktree changes now, and outside this refactor. One half has a known cause: the thread's fixture assumes a pass sends at most `quinn_udp::BATCH_SIZE` transmits, but that constant is 1 on macOS and Windows. The other half, what the frame allocates on macOS, needs a macOS host to find.

| Option | What it does | Cost |
|---|---|---|
| A. Fix it on the `net` branch (recommended) | The transport's own work takes R5: the fixture delivers a pass over as many reads as it needs, and the macOS allocation is traced on the laptop. | This branch stays red off Linux until then, as master is. |
| B. Fix it here | The same work on this branch. | Changes the transport outside the refactor, beside the `net` branch's changes; needs access to the macOS laptop. |

**Blocked.** A green CI run on this branch.
