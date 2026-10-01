# Stats and modifiers

How a unit's numbers and states come from its type, its level and the modifiers it carries. How a unit gains levels is [Progression](progression.md)'s. The model follows Unreal's Gameplay Ability System where the field agrees: a base value from the type and level, a current value its modifiers change, stacks counted by source. The rules for movement and health follow League of Legends, where they are exact.

## State and derived

- **State**, hashed, saved, restored and replicated: each unit's modifiers, its level, and the current amount of each pool (health, resource).
- **Derived**, never state: each unit's stats and states. They are computed from the state again whenever its modifiers, level or type change, before any system reads them, so the server, a client after a rollback and a replay see the same numbers. The components that hold a stat's effect, a unit's step a tick, its attack's period and damage and its pools' maxima, take their numbers from the derived stats each time they change; those components exist from the unit's spawn, as state, so their numbers enter the hash as well.

## Stats

The mode declares each stat in `data/mode.toml`, with how it combines and its limits:

```toml
[stats.armor]
combine = "sum"       # or "highest"
min = "-100"          # optional, both
max = "500"
```

A unit's value of a stat is its type's value at its level, `base + per_level × (level − 1)`, combined with its modifiers' values for it:

- `sum`: the type's value plus each modifier's value times its stacks. Fixed point adds exactly, so the order of modifiers never matters.
- `highest`: the greatest of the type's value and each modifier's value times its stacks, as a slow where only the strongest counts.

Then the limits clamp it. A sum that leaves the range of a number stops at its end before the limits.

The engine reads some stats by name and fixes how it uses them. The mode declares them like the others, so it sets their limits:

| Stat | Engine use |
| --- | --- |
| `health`, `resource` | The maximum of the pool |
| `health_regen`, `resource_regen` | Added to the pool each second: regen ÷ tick rate a tick, the remainder carried, so a second gains exactly the regen |
| `move_speed`, `move_speed_pct`, `slow` | Move speed is `move_speed × (1 + move_speed_pct) × (1 − slow)`, rounded once, within `move_speed`'s limits and the manifest's `max_move_speed`; `slow_immune` takes the slow as 0 |
| `attack_speed`, `attack_speed_pct` | Attacks a second: `attack_speed × (1 + attack_speed_pct)`, exactly, within `attack_speed`'s limits; the period is the tick rate over that, rounded up, and at least a tick longer than the windup, as a windup in milliseconds does not shrink with speed |
| `attack_damage` | The damage of an attack |

The damage system adds its own: `crit_chance`, `life_steal`, `spell_vamp`, `healing_received_pct`.

**Pools.** When a pool's maximum rises, its current amount rises by as much; when the maximum falls, the current amount stays, unless it is now above the maximum. This is League of Legends' rule; Dota 2 keeps the fraction instead, which needs a rounding the engine does not take.

## Modifiers

A modifier is an instance on its carrier, from a source: the unit whose ability, attack or aura applied it, or none for one the mode applies. A carrier holds at most one instance of an id from each source, as the Gameplay Ability System's stacking by source does.

- **Applying.** `ctx.add_modifier(unit, id)` and `(unit, id, duration_ms)` return the instance's handle. When the instance exists, `reapply` decides: `refresh` sets its duration whole again; `stack` adds a stack, up to `max_stacks`, and sets the duration whole again; `ignore` leaves it. Either way the handle of the instance comes back.
- **Stacks.** With `stacks_expire_ms`, each stack keeps its own end, and the instance ends with its last stack. A stack past `max_stacks` takes the place of the stack that ends soonest. A script may write `m.stacks`: the stacks it removes are those that end soonest, and those it adds end as stacks applied in that tick do. At 0 the instance stays, with no stats.
- **Time.** A duration counts in ticks, rounded up and at least one, from the end of the tick it was applied in: a modifier of `d` ticks applied in tick `t` holds through every stage of tick `t + d`, and ends as tick `t + d + 1` starts, as does a stack that ends on its own.
- **Ending.** `ctx.remove(handle)` ends it at once; so does a spent shield, and the carrier's death, except for a passive.
- **Passives.** An ability's `passive_modifier` is an instance from the carrier itself while the ability has a rank; with `passive_while_ready`, only while it is off cooldown. An avatar's `passive` is an instance from the avatar itself from its spawn on, with no ability, at rank 1. A passive keeps its stacks and state across a death. A passive whose stacks end one by one counts something, as Twin Cut counts attacks: it starts with no stacks, and it stays when its last stack ends.
- **Auras.** A modifier with an `aura` makes its carrier a source: in Resolve each tick, every living unit within the aura's radius that `affects` selects holds the aura's modifier from that carrier, with no duration, and loses it the tick it leaves. There is no linger: the sim has no flicker to hide.
- **Order.** Instances are kept by id, then source, so every walk over them is in the same order on every machine.
- **Ids by package.** A modifier's id is its name within its package, the mode or a package it depends on; a script names the modifiers of its own package, so two packages may each have a `haste`.
- **Numbers.** A modifier's number fields, its duration, interval, stack expiry, shield, aura radius and `stats`, resolve when it is applied, and again when it is refreshed or stacked: a `{ param }` reads the modifier's own params, then those of the ability that applied it, at that ability's rank on the source, or rank 1 with none. A rank or stat that changes later changes the next application, not one held, as the Gameplay Ability System captures a magnitude when it applies an effect.
- **Its ability.** An instance keeps the ability whose cast, projectile, area or modifier applied it, or whose passive it is; one the mode applies has none.
- **Handles.** `ctx.add_modifier` returns the handle of the instance it will add or reapply. A call that applies one instance twice gets the same handle twice, which shows both applications; after `ctx.remove`, a new application starts it again from its defaults. Writes to a handle's `stacks` and `state` in a call apply when the call ends, after its adds, and the call reads them back; a failed call writes none. A modifier's `[state]` starts at its defaults when it is added and is kept when it is refreshed or stacked.

Intervals, shields and the events modifier scripts hear are part of [Combat](combat.md#damage-and-death): the damage system runs them.

## States

A state is a flag a modifier puts its carrier in, as Dota 2's modifier states and the Gameplay Ability System's tags are. A unit's states are derived, never state: the union of its modifiers' `states`, whatever their stacks, and of its unit type's, computed in the same refresh as its stats, so every system reads one set. An instance keeps the states of its modifier, as it keeps its stats, so a snapshot and a client hold them with it.

| State | Effect |
| --- | --- |
| `stunned`, `airborne` | No moving, attacking or casting; an attack in its windup, a cast in its cast time and a channel stop |
| `rooted` | No moving |
| `silenced` | No casting; a cast in its cast time and a channel stop |
| `disarmed` | No attacking; an attack in its windup stops |
| `slow_immune` | The slow stat counts as 0 |
| `stealthed` | Enemies see the unit only through a unit with `true_sight` that sees its cell ([Vision](vision.md#stealth)) |
| `true_sight` | The unit's sight shows stealthed enemies; a unit type's `true_sight` gives it always |
| `untargetable` | `find`, `find_visible` and `nearest_visible` skip it; an attack order and a cast aimed at it are refused; a chaser drops it; a projectile homing on it is lost; areas still reach it |
| `invulnerable` | As `untargetable`, and it takes no damage, from areas either: Dota 2 and League of Legends protect a structure so |

- **One question each.** The set answers four questions, and each system asks only its own: may the unit move (the Move stage), attack (combat's attack and strike), cast (the casts' start and resolve), and is it a target (combat's `Targets`, the script view's queries, the damage pass). No system reads modifiers for a state.
- **Stopped, not dropped.** A state that stops an action keeps the order behind it: the destination and route, the attack target, the ordered cast. The action runs again when the state ends, as League of Legends buffers input. A windup or a cast time it interrupts starts again from nothing; an interrupted cast spends no cost and no cooldown, as Dota 2's cast point does. A cast its checks refuse for another reason is dropped.
- **Engine modifiers.** `ctx.stun(unit, ms)`, `ctx.slow(unit, fraction, ms)` and `ctx.knock_up(unit, ms)` apply the engine's own modifiers, `stun`, `slow` and `knock_up`, from the acting unit, for `ms`. The engine loads them into the book under a package of its own, after every package, so they refresh, stack, end, replicate and hash as any other: two stuns from one source refresh, and from two the longer holds. `slow` adds `fraction` to the slow stat, which the mode must declare for a script that calls it; `knock_up` makes its unit `airborne` and does not move it, as the sim has no height.

## What a client predicts

A client keeps its own units' modifiers as the server sends them and runs their time, so a modifier ends on the client in the tick it ends on the server; it derives their stats and states as the server does, so its predicted movement and attacks obey them. It creates no modifier: those come from scripts, which only the server runs, so a stun the client learns late costs a correction, as the Gameplay Ability System also predicts no effect it did not start.

To derive them, a client builds the mode's stat book from the packages it holds, with the unit types numbered as the server numbers them, by one order both read from the packages; the server sends each unit's type once and its level as it changes. A unit's step, its attack's period and damage, and its pools' maxima are then derived on both sides, and the server no longer sends the step.
