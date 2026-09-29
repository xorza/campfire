# MOBA kit

## Units and orders

**A unit** is kit components (position, collision shape, health, team, stats, modifiers, abilities, current order) plus script data: serializable key-values, part of the game state.

**Input format: orders.** Players, bots and AI all issue the same orders: move, attack, cast (ability, target), stop, hold. The kit executes them (pathfinding, attacking, casting).

## AI

AI scripts drive creeps, neutrals and bosses.

- Set per unit type in its data. `think(unit)` runs every N ticks (e.g. every 0.25 s) and issues orders.
- Think times are staggered by unit id, so units do not all think on the same tick.
- Only game queries (units in radius, nearest enemy, is visible) and the sim RNG; results are sorted by unit id.
- The reference game ships lane-creep and camp-leash AI as ordinary scripts.

## Abilities

The kit handles the mechanics; the script only describes the effect.

- **Data:** cooldown, cost (resource and amount), range, targeting (none, unit, point, direction, area), cast time, channel time, levels.
- **Kit:** validates every cast (range, cooldown, cost, valid and visible target), runs cast and channel time, handles interrupts, applies cooldown and cost.
- **Script hooks:** `on_cast` when the cast completes, `on_channel_tick`, `on_projectile_hit`, and modifier hooks `on_apply`, `on_tick`, `on_expire`.

| Primitive | Does |
| --- | --- |
| Projectile | Linear or homing; calls `on_projectile_hit` |
| Area | Circle, cone or line, instant or lasting; hits units inside |
| Modifier | Buff or debuff: duration, stacks, stat changes, states (stun, silence, root, invisible), periodic tick |
| Damage | Amount and type; the final formula (armor, resistances) is the mode hook `calc_damage` |

## Pathfinding and vision

- Grid backend: A* for the long route; shared local steering avoids units and allows body blocking.
- Creep lanes are waypoints in map data.
- Vision: grid fog of war; each unit reveals cells within its sight range, blocked by terrain.

## Network defaults

| Data | Sent to |
| --- | --- |
| Position, health, visible modifiers, current animation | Everyone who sees the unit |
| Mana, cooldowns, resources such as gold | Owner, or team (set by the mode) |

Clients predict their own hero's movement and cast start; damage and deaths appear only when the server confirms them.

## Tick steps

One MOBA tick, in fixed order (script steps marked):

1. **Apply orders** (kit): player and bot orders for this tick.
2. **AI think** (script): units due this tick issue orders.
3. **Execute orders** (kit): pathfinding, steering, attacks, cast start.
4. **Collision** (core): resolve overlaps with the mode's backend.
5. **Ability effects** (script): `on_cast`, projectiles, areas.
6. **Modifiers and damage** (kit + script): modifier ticks; damage via the mode's `calc_damage`.
7. **Deaths and timers** (script): `on_unit_died`, `on_timer`; `match.end` ends the match.
8. **Vision** (core): update grid fog of war for each team.
9. **Record and send** (core): log orders; send each client its visible state and events.

## Examples

Function names show the shape of the API. Scripts have no decimal literals (`no_float`): decimal values come from data files, time is in milliseconds.

**Mode: creep waves, bounties, win condition** (`scripts/mode.rhai`)

```rhai
fn on_match_start(m) {
    m.timer("creep_wave", 30000, true);        // every 30 s, repeating
}

fn on_timer(m, name) {
    if name == "creep_wave" {
        for lane in m.map.lanes { m.spawn_wave(lane, "melee_creep", 3); }
    }
}

fn on_unit_died(m, unit, killer) {
    if unit.has_tag("core") { m.end(m.enemy_team(unit.team)); }
    if killer != () { m.add_resource(killer.owner, "gold", unit.data.bounty); }
}
```

**Ability: fireball** (`abilities/fireball.toml` + `scripts/fireball.rhai`)

```toml
cooldown_ms = 8000
cost = { resource = "mana", amount = 90 }
range = "7.5"
targeting = "unit"
cast_time_ms = 250
script = "scripts/fireball.rhai"
speed = "12"
damage = 120
stun_ms = 1000
```

```rhai
fn on_cast(ctx, caster, target) {
    ctx.projectile_homing(caster, target, ctx.data.speed);
}

fn on_projectile_hit(ctx, proj, target) {
    ctx.damage(proj.source, target, ctx.data.damage, "magic");
    ctx.modifier(target, "stunned", ctx.data.stun_ms);
}
```

**Creep AI** (`scripts/creep_ai.rhai`, unit data sets `think_ms = 250`)

```rhai
fn think(ctx, unit) {
    let enemy = ctx.nearest_enemy(unit, unit.data.aggro_range);
    if enemy != () { unit.order_attack(enemy); } else { unit.order_follow_lane(); }
}
```
