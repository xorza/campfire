# Campfire — Script API (MOBA kit)

Draft, derived from the reference packages in `source/packages/moba/`: six heroes, the player spells and the 3v3 mode. Every name below is used there; a name the packages do not need is not here. Nothing runs yet.

## Rules

- **Hooks by name.** The kit calls a script function by its hook name; a missing hook does nothing. Every hook takes `ctx` first.
- **Handles read, `ctx` changes.** Handles (units, modifiers, projectiles, areas) are read-only, except `.state`, which a call may write. Every other change goes through `ctx` and is queued: the call is all-or-nothing ([Scripting](02-engine-core.md#scripting)).
- **New entities at once.** A handle returned by a `ctx` call that creates an entity is usable in the same call, including `.state`; the entity appears when the call commits.
- **Event order.** A call's effects apply in call order. The hooks those effects trigger are queued first-in first-out and run after all of the call's effects. A chain longer than an engine constant fails with `script_error`.
- **Queries.** Lists are sorted by stable id; `nearest_*` sorts by distance, then id. `*_near` includes units hidden from the caller's team, for effects that hit everything in an area. `visible_*` returns only what that team sees, for choosing a target.
- **Numbers.** [Game Scripting](03-game-scripting.md#numbers). `num(i)` makes a `Num` from an integer. `Num` has `min`, `max`, `clamp`, `round`, `floor`, `ceil`; `round` and the others return integers.
- **Randomness** comes only from `ctx.chance` and `ctx.pick`, both from the secret stream. Crits are rolled by the kit.
- **Time** is in milliseconds, rounded up to whole ticks. `ctx.now_ms()` is the match time.
- **Rhai names.** Rhai's reserved words cannot be names, including `spawn` and `match`.

## Data files

Arrays in kit fields and in the params of an ability, or of a modifier an ability applies, are per rank. Anywhere else an array is a list.

**Param values** (`[params]`, read with `ctx.value`):

| Form | Example | Value |
| --- | --- | --- |
| Integer | `slow_ms = 2000` | the integer |
| Decimal string | `radius = "3.0"` | a `Num` |
| Array | `slow = ["0.15", "0.20"]` | the entry for the current rank |
| Scaling table | `{ base = [80, 120], ap = "0.65" }` | `base + per_level × level + Σ ratio × stat`, with the caster's stats |

Scaling keys: `base`, `per_level`, and a stat ratio each for `ad`, `bonus_ad`, `ap`, `max_health`, `bonus_health`, `armor`, `magic_resist`.

**Hero** (`data/hero.toml`): `name`, `role`, `resource` (`mana` or `energy`), `passive` (a modifier id), `slots` (four ability ids; the last is the ultimate), `[attack]` (`range`, `windup_ms`, `projectile_speed`; no speed means melee), `[stats]` (`stat = { base, per_level }`, the value at level `n` is `base + per_level × (n − 1)`), `[state]`, `[abilities.<id>]`, `[modifiers.<id>]`.

**Ability** kit fields:

| Field | Meaning |
| --- | --- |
| `script` | Script file |
| `targeting` | `none`, `enemy`, `ally`, `unit`, `point`, `direction` |
| `range`, `cooldown_ms`, `cost`, `cast_time_ms` | Cost is in the hero's resource |
| `clamp_to_range` | A target beyond range is moved in, instead of the caster walking |
| `toggle` | `{ cost_per_attack }` or `{ cost_per_second }`; calls `on_toggle` |
| `channel` | `{ duration_ms, tick_ms }`; starts after `on_cast` |
| `charges` | `{ max, recharge_ms }` |
| `recast_ms` | Window for `on_recast`; the cooldown starts when it closes |
| `passive_modifier` | Modifier held while the ability has a rank |
| `passive_while_ready` | Hold it only while the ability is off cooldown |
| `[params]`, `[projectile_state]` | Script values; state of the projectiles this ability fires |

**Modifier** fields: `script`, `duration_ms` (absent: until removed), `interval_ms`, `max_stacks`, `stacks_expire_ms`, `stats` (per stack), `states`, `shield` (the modifier ends when the shield is spent), `aura` (`{ radius, affects, modifier }`), `[params]`, `[state]`. A number field may be `{ param = "<name>" }`, resolved like `ctx.value`.

**Mode** (`data/mode.toml`): `script`, `[state]`, `[params]`, `[modifiers]`. **Units** (`data/units.toml`): `[units.<id>]` with `ai`, `think_ms`, `tags`, `true_sight`, `attack`, `stats`, `params`.

**State types:** `int`, `num`, `bool`, `string`, `entity`, `entity_list`, `pos`, `vec`; each with `default` and, for mode and hero state, `sync`.

**Stats:** `health`, `health_regen`, `resource`, `resource_regen`, `attack_damage`, `attack_damage_pct`, `ability_power`, `ability_power_pct`, `attack_speed`, `attack_speed_pct`, `crit_chance`, `armor`, `magic_resist`, `armor_pen`, `armor_pen_pct`, `magic_pen`, `magic_pen_pct`, `move_speed`, `move_speed_pct`, `life_steal`, `spell_vamp`, `cooldown_reduction`, `physical_block`, `healing_received_pct`, `damage_dealt_pct`.

**States:** `stunned`, `rooted`, `silenced`, `disarmed`, `airborne`, `stealthed`, `untargetable`, `slow_immune`.

## Handles

| Handle | Reads |
| --- | --- |
| Unit | `pos`, `team`, `owner`, `level`, `alive`, `is_hero`, `targetable`, `target`, `attack_range`, `health`, `max_health`, `stat(name)`, `params`, `state`, `spawn_pos`, `lane`, `unit_type`, `has_tag(tag)`, `has_modifier(id)`, `is_enemy_of(unit)`, `can_see(unit)`, `recent_attackers(ms)` |
| Modifier `m` | `carrier`, `source`, `stacks`, `state` |
| Projectile | `source`, `pos`, `distance` flown, `state` |
| Area | `source`, `pos` |
| Attack hit | `damage`, `crit`, `extra` (from `ctx.attack_hit`) |
| Damage `dmg` | `amount`, `kind`, `attack`, `ability` (`""` when none) |
| Position, vector | `distance_to`, `direction_to`, `rotated_deg`, `+`, `-`, `*` |

Heroes carry the tag `hero`.

## `ctx`

| Group | Calls |
| --- | --- |
| Values | `value(name)`, `range`, `now_ms()`, `state`, `map` (`lanes`, `neutral_spawns`), `teams` |
| Queries | `enemies_near(of, pos, r)`, `allies_near(of, pos, r)`, `visible_enemies_near(of, pos, r)`, `enemy_heroes_near(of, pos, r)`, `nearest_visible_enemy(of, r, tag)`, `heroes()`, `heroes_of(team)`, `units_tagged(tag)`, `enemy_team(team)`, `hero_available(player, id)` |
| Random | `chance(p)`, `pick(list)` |
| Combat | `damage(source, target, amount, kind)` with kind `physical`, `magic` or `true`; `heal(unit, amount)`; `restore(unit, resource, amount)`; `attack_hit(attacker, target)` |
| Modifiers | `add_modifier(unit, id)` or `(unit, id, duration_ms)`, returns the modifier; `remove_modifier(unit, id)`; `add_stacks(m, n)`; `set_stacks(m, n)` |
| Crowd control | `stun(unit, ms)`, `slow(unit, fraction, ms)`, `knock_up(unit, ms)`, `knock_back(unit, from, distance, ms)` |
| Movement | `dash(unit, to, #{ speed })`, `teleport(unit, pos)` |
| Projectiles | `projectile_line(source, from, dir, opts)`, `projectile_homing(source, from, target, opts)`, `projectile_to(source, from, pos, opts)`; opts: `speed`, `range`, `width`, `stop_on_hit`, `once_per_cast`, `hits` (`enemies` by default, `enemy_heroes`, `all`), `sight_radius`, `collide` |
| Areas | `area_circle(source, pos, opts)`; opts: `radius`, `delay_ms`, `duration_ms`, `interval_ms`, `affects` (`enemies`, `allies`, `all`) |
| Vision | `reveal(for_unit, pos, radius, ms)` |
| Abilities | `reduce_cooldown(unit, id, ms)`, `reduce_cooldowns(unit, fraction)` (basic abilities), `add_charge(unit, id)` |
| Progress | `add_resource(player, name, amount)`, `add_xp(hero, amount)` |
| Orders (AI) | `order_attack(unit, target)`, `order_move(unit, pos)`, `order_follow_lane(unit)`, `order_reset(unit)` (walk home, heal, drop target) |
| Mode | `timer(name, ms, repeat, data)`, `end(result)`, `spawn_heroes()`, `spawn_unit(type, team, pos)`, `spawn_wave(team, lane, types)`, `respawn(unit, ms)`, `choose_hero(player, id)`, `choose_spells(player, ids)` |

`ctx.value` reads: in an ability, its params at the caster's rank and current stats; in a modifier, the modifier's params and then those of the ability that applied it, resolved once when the modifier is applied; in a mode or AI script, the mode's params. `unit.params` gives a unit type's params unresolved.

## Hooks

| Script | Hooks |
| --- | --- |
| Mode | `on_match_start(ctx)`, `on_mode_input(ctx, player, name, value)`, `on_timer(ctx, name, data)`, `on_player_join(ctx, player)`, `on_player_leave(ctx, player)`, `on_unit_died(ctx, unit, killer)`, `calc_damage(ctx, source, target, amount, kind)` |
| Ability | `on_cast(ctx, caster, target)` (target is a unit, a position or `()`), `on_toggle(ctx, caster, on)`, `on_recast(ctx, caster, timed_out)`, `on_channel_tick(ctx, caster)`, `on_channel_end(ctx, caster, interrupted)`, `on_dash_end(ctx, unit, target)`, `on_projectile_hit(ctx, proj, target)`, `on_projectile_end(ctx, proj)`, `on_area_trigger(ctx, area, units)`, `on_area_tick(ctx, area, units)` |
| Modifier | `on_apply(ctx, m)`, `on_interval(ctx, m)`, `on_expire(ctx, m)`, and the carrier's events: `on_attack(ctx, m, target)`, `on_attack_hit(ctx, m, target, hit)`, `on_damage_dealt(ctx, m, target, dmg)`, `on_damage_taken(ctx, m, source, dmg)`, `on_kill(ctx, m, victim)`, `on_takedown(ctx, m, victim)` |
| AI | `think(ctx, unit)` |

A hero passive is a modifier the hero always carries, so it uses the modifier hooks. `calc_damage` returns the final amount and may not queue effects.

## Found while writing the scripts

What the earlier design lacked, now part of the kit or the engine:

1. Passives are permanent modifiers, and carrier events go to modifier scripts. One mechanism covers passives, buffs, marks and items.
2. Abilities can grant a passive modifier, also one held only while the ability is ready.
3. Toggles, channels, charges and recasts are kit fields, not script code.
4. Modifiers, projectiles and heroes need state schemas, not only units and the mode.
5. Handles are read-only except `.state`; entities created in a call are usable in that call.
6. Effects trigger hooks in first-in first-out order, with a chain limit.
7. Area queries include hidden units; target choice uses visible-only queries.
8. `calc_damage` is a hook that returns a value and has no effects.
9. `on_timer` takes a data value, and the mode needs `on_mode_input` for hero and spell choice.
10. Rhai's default limits differ between debug and release builds, so the engine sets every limit itself ([Scripting](02-engine-core.md#scripting)).
11. `spawn` is reserved in Rhai.
12. A package in the workspace names its dependencies by path; the built package names them by fingerprint.

## Open questions

- [ ] Modifier values resolve once when applied; confirm, or resolve them live.
- [ ] Camp monsters respawn one by one; whole camps may be better.
- [ ] Kill streaks and assist rules beyond an even gold split.
- [ ] Items: the same modifier and ability model, not yet written.
